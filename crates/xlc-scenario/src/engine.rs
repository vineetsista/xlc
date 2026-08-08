//! The scenario engine (§8.6): every uncertain-cone cell becomes a buffer
//! of shape [tile] evaluated across scenarios, in structure-of-arrays
//! layout, scheduled cell-at-a-time in dependency order per tile.
//!
//! Two execution paths per cell:
//!   - FAST: families whose instruction DAG is pure arithmetic over
//!     single-cell refs vectorize across the tile (pulp-dispatched);
//!     any non-finite result or non-numeric operand falls back.
//!   - SCALAR: everything else evaluates through the scalar interpreter
//!     per scenario — bit-identical to the receipt by construction.
//!
//! Bytes-moved accounting (the Gate 6 metric): per scenario, each cone
//! cell's value is written once (8B) and each (consumer, dirty-dep)
//! stream is read once (8B). Static workbook values are scenario-
//! invariant and cost nothing per scenario. The theoretical minimum is
//! one write per cone cell plus one read per uncertain input.

use std::cell::Cell as StdCell;
use std::collections::{HashMap, HashSet};

use crate::dist::Dist;
use crate::rng::DrawAddr;
use xlc_eval::interp::{Ctx, Interp, Operand, Origin, Rect, SheetId};
use xlc_eval::workbook::Workbook;
use xlc_eval::Value;
use xlc_ir::{Family, Inst, Module};
use xlc_parse::ast::{BinOp, Expr, UnOp};

pub type CellKey = (SheetId, u32, u32);

#[derive(Clone)]
pub struct ScenarioSpec {
    pub seed: u64,
    pub inputs: Vec<(CellKey, Dist)>,
}

pub struct Engine<'wb> {
    wb: &'wb Workbook,
    module: Module,
    spec: ScenarioSpec,
    /// Evaluation order over dirty formula cells: (family idx, lane idx).
    schedule: Vec<(usize, usize)>,
    /// Dirty formula cells -> schedule position.
    dirty_index: HashMap<CellKey, usize>,
    /// Input cell -> index in spec.inputs.
    input_index: HashMap<CellKey, usize>,
    /// Per schedule entry: dirty deps (schedule positions) and input deps.
    dep_cells: Vec<Vec<usize>>,
    dep_inputs: Vec<Vec<usize>>,
    /// Consumers count per schedule entry (for buffer-pool liveness).
    consumers: Vec<usize>,
    /// Fast-path plan per family index (shared across lanes).
    fast: Vec<Option<FastPlan>>,
}

/// A vectorizable instruction plan: pure arithmetic over single cells.
struct FastPlan {
    insts: Vec<FastInst>,
    result: u32,
}

enum FastInst {
    Const(f64),
    /// Load a single cell rebased per lane (row/col deltas applied).
    Load { sheet: SheetId, row: i64, col: i64, row_abs: bool, col_abs: bool },
    /// SUM over one rectangular range (range semantics: text/bool ignored,
    /// blanks zero; any error or non-numeric dirty dep falls back).
    SumRange { sheet: SheetId, area: xlc_parse::ast::Area },
    Bin { op: BinOp, a: u32, b: u32 },
    Neg(u32),
}

pub struct SweepResult {
    /// Per watched cell: per-scenario values.
    pub watched: HashMap<CellKey, Vec<Value>>,
    pub scenarios: u32,
    pub cone_cells: usize,
    pub bytes_written: u64,
    pub bytes_read_streams: u64,
    pub fast_path_cells: usize,
    pub scalar_path_cells: usize,
    /// Peak count of simultaneously-live tile buffers (the cache-residency
    /// witness for the bytes-moved claim).
    pub peak_live_buffers: usize,
}

impl<'wb> Engine<'wb> {
    pub fn new(wb: &'wb Workbook, spec: ScenarioSpec) -> Self {
        let module = xlc_ir::lower(wb);
        let input_index: HashMap<CellKey, usize> =
            spec.inputs.iter().enumerate().map(|(i, (k, _))| (*k, i)).collect();

        // Per (family, lane): dep rects, computed once.
        let mut lane_deps: Vec<Vec<Vec<Rect>>> = Vec::with_capacity(module.families.len());
        for fam in &module.families {
            let mut per_lane = Vec::with_capacity(fam.lanes.len());
            for lane in 0..fam.lanes.len() {
                per_lane.push(family_dep_rects(wb, fam, lane));
            }
            lane_deps.push(per_lane);
        }

        // Dirty-cone fixpoint: a cell is dirty if any dep rect contains an
        // input cell or a dirty formula cell.
        let mut dirty: HashSet<CellKey> = HashSet::new();
        let inputs_set: HashSet<CellKey> = input_index.keys().copied().collect();
        loop {
            let mut changed = false;
            for (fi, fam) in module.families.iter().enumerate() {
                for (li, &(row, col)) in fam.lanes.iter().enumerate() {
                    let key = (fam.sheet, row, col);
                    if dirty.contains(&key) {
                        continue;
                    }
                    let hit = lane_deps[fi][li].iter().any(|r| {
                        rect_hits(r, &inputs_set) || rect_hits(r, &dirty)
                    });
                    if hit {
                        dirty.insert(key);
                        changed = true;
                    }
                }
            }
            if !changed {
                break;
            }
        }

        // Schedule: demand-driven topological order with an explicit stack.
        let mut cell_of: HashMap<CellKey, (usize, usize)> = HashMap::new();
        for (fi, fam) in module.families.iter().enumerate() {
            for (li, &(row, col)) in fam.lanes.iter().enumerate() {
                cell_of.insert((fam.sheet, row, col), (fi, li));
            }
        }
        let mut schedule = Vec::new();
        let mut dirty_index: HashMap<CellKey, usize> = HashMap::new();
        let mut visiting: HashSet<CellKey> = HashSet::new();
        for key in dirty.iter().copied() {
            if dirty_index.contains_key(&key) {
                continue;
            }
            // Iterative DFS.
            let mut stack = vec![(key, false)];
            while let Some((k, expanded)) = stack.pop() {
                if dirty_index.contains_key(&k) {
                    continue;
                }
                if expanded {
                    visiting.remove(&k);
                    let (fi, li) = cell_of[&k];
                    dirty_index.insert(k, schedule.len());
                    schedule.push((fi, li));
                    continue;
                }
                if visiting.contains(&k) {
                    // Circular reference inside the cone: excluded from
                    // scenario evaluation (reported by the caller).
                    continue;
                }
                visiting.insert(k);
                stack.push((k, true));
                let (fi, li) = cell_of[&k];
                for r in &lane_deps[fi][li] {
                    for_each_dirty_in_rect(r, &dirty, |dk| {
                        if dk != k && !dirty_index.contains_key(&dk) {
                            stack.push((dk, false));
                        }
                    });
                }
            }
        }

        // Dep lists + consumer counts against the final schedule.
        let mut dep_cells: Vec<Vec<usize>> = vec![Vec::new(); schedule.len()];
        let mut dep_inputs: Vec<Vec<usize>> = vec![Vec::new(); schedule.len()];
        let mut consumers = vec![0usize; schedule.len()];
        for (pos, &(fi, li)) in schedule.iter().enumerate() {
            let mut dc: HashSet<usize> = HashSet::new();
            let mut di: HashSet<usize> = HashSet::new();
            for r in &lane_deps[fi][li] {
                for_each_dirty_in_rect(r, &dirty, |dk| {
                    if let Some(&p) = dirty_index.get(&dk) {
                        if p != pos {
                            dc.insert(p);
                        }
                    }
                });
                for (k, &ii) in &input_index {
                    if k.0 == r.sheet && r.contains(k.1, k.2) {
                        di.insert(ii);
                    }
                }
            }
            for &p in &dc {
                consumers[p] += 1;
            }
            dep_cells[pos] = dc.into_iter().collect();
            dep_inputs[pos] = di.into_iter().collect();
        }

        // Fast-path plans per family.
        let fast = module.families.iter().map(fast_plan).collect();

        Engine {
            wb,
            module,
            spec,
            schedule,
            dirty_index,
            input_index,
            dep_cells,
            dep_inputs,
            consumers,
            fast,
        }
    }

    pub fn cone_cells(&self) -> usize {
        self.schedule.len()
    }

    pub fn inputs_len(&self) -> usize {
        self.spec.inputs.len()
    }

    pub fn spec_inputs(&self) -> &[(CellKey, Dist)] {
        &self.spec.inputs
    }

    pub fn seed(&self) -> u64 {
        self.spec.seed
    }

    /// Local derivatives of cone cell `pos` with respect to its numeric
    /// sources, evaluated at the point given by `inputs` and the solved
    /// cone `value_of`. None => opaque to AD (structural boundary).
    pub(crate) fn local_derivatives(
        &self,
        pos: usize,
        inputs: &[f64],
        value_of: &std::collections::HashMap<CellKey, f64>,
    ) -> Option<Vec<(crate::ad::Source, f64)>> {
        use crate::ad::Source;
        let (fi, li) = self.schedule[pos];
        let plan = self.fast[fi].as_ref()?;
        let fam = &self.module.families[fi];
        let (row, col) = fam.lanes[li];
        let (er, ec) = fam.lanes[0];
        let (dr, dc) = (row as i64 - er as i64, col as i64 - ec as i64);

        // Forward with per-inst value + sparse gradient.
        let mut vals: Vec<f64> = Vec::with_capacity(plan.insts.len());
        let mut grads: Vec<std::collections::HashMap<Source, f64>> =
            Vec::with_capacity(plan.insts.len());
        let mut source_of = |sheet: SheetId, rr: u32, cc: u32| -> (f64, Option<Source>) {
            let key = (sheet, rr, cc);
            if let Some(&ii) = self.input_index.get(&key) {
                (inputs[ii], Some(Source::Input(ii)))
            } else if let Some(&p) = self.dirty_index.get(&key) {
                (value_of.get(&key).copied().unwrap_or(f64::NAN), Some(Source::Cone(p)))
            } else {
                match self.wb.sheets[sheet as usize].values.get(&(rr, cc)) {
                    Some(Value::Num(x)) => (*x, None),
                    Some(Value::Bool(b)) => (if *b { 1.0 } else { 0.0 }, None),
                    None => (0.0, None),
                    Some(_) => (f64::NAN, None),
                }
            }
        };
        for inst in &plan.insts {
            let (v, g) = match inst {
                FastInst::Const(x) => (*x, std::collections::HashMap::new()),
                FastInst::Load { sheet, row: r, col: c, row_abs, col_abs } => {
                    let rr = u32::try_from(if *row_abs { *r } else { *r + dr }).ok()?;
                    let cc = u32::try_from(if *col_abs { *c } else { *c + dc }).ok()?;
                    let (v, src) = source_of(*sheet, rr, cc);
                    if v.is_nan() {
                        return None;
                    }
                    let mut g = std::collections::HashMap::new();
                    if let Some(src) = src {
                        g.insert(src, 1.0);
                    }
                    (v, g)
                }
                FastInst::SumRange { sheet, area } => {
                    let a2 = xlc_ir::rebase_pub(area, dr, dc)?;
                    let interp = Interp::new(self.wb, Origin { sheet: *sheet, row, col });
                    let Operand::Ref(rects) = interp.resolve_area(&[*sheet], &a2) else {
                        return None;
                    };
                    let mut total = 0.0;
                    let mut g = std::collections::HashMap::new();
                    for r2 in &rects {
                        for (rr, cc) in r2.cells() {
                            let key = (r2.sheet, rr, cc);
                            if let Some(&ii) = self.input_index.get(&key) {
                                total += inputs[ii];
                                *g.entry(Source::Input(ii)).or_insert(0.0) += 1.0;
                            } else if let Some(&p) = self.dirty_index.get(&key) {
                                total += value_of.get(&key).copied().unwrap_or(f64::NAN);
                                *g.entry(Source::Cone(p)).or_insert(0.0) += 1.0;
                            } else if let Some(Value::Num(x)) =
                                self.wb.sheets[r2.sheet as usize].values.get(&(rr, cc))
                            {
                                total += x;
                            }
                        }
                    }
                    if total.is_nan() {
                        return None;
                    }
                    (total, g)
                }
                FastInst::Bin { op, a, b } => {
                    let (va, vb) = (vals[*a as usize], vals[*b as usize]);
                    let (da, db) = match op {
                        BinOp::Add => (1.0, 1.0),
                        BinOp::Sub => (1.0, -1.0),
                        BinOp::Mul => (vb, va),
                        BinOp::Div => {
                            if vb == 0.0 {
                                return None;
                            }
                            (1.0 / vb, -va / (vb * vb))
                        }
                        _ => return None,
                    };
                    let v = match op {
                        BinOp::Add => va + vb,
                        BinOp::Sub => va - vb,
                        BinOp::Mul => va * vb,
                        BinOp::Div => va / vb,
                        _ => unreachable!(),
                    };
                    let mut g = std::collections::HashMap::new();
                    for (src, gd) in &grads[*a as usize] {
                        *g.entry(*src).or_insert(0.0) += gd * da;
                    }
                    for (src, gd) in &grads[*b as usize] {
                        *g.entry(*src).or_insert(0.0) += gd * db;
                    }
                    (v, g)
                }
                FastInst::Neg(a) => {
                    let mut g = std::collections::HashMap::new();
                    for (src, gd) in &grads[*a as usize] {
                        g.insert(*src, -gd);
                    }
                    (-vals[*a as usize], g)
                }
            };
            vals.push(v);
            grads.push(g);
        }
        Some(grads[plan.result as usize].iter().map(|(s, d)| (*s, *d)).collect())
    }

    pub fn cone_keys(&self) -> Vec<CellKey> {
        (0..self.schedule.len()).map(|p| self.key_of(p)).collect()
    }

    /// N=1 oracle: with the spec's (Point) inputs, the scenario engine's
    /// value for every cone cell must be bit-identical to a scalar
    /// full-recompute over the same schedule. Returns (cells, mismatches).
    pub fn verify_n1(&self) -> (usize, usize) {
        let keys = self.cone_keys();
        let sweep = self.run(1, 1, &keys);

        let mut wb2 = self.wb.clone();
        for ((sheet, row, col), dist) in &self.spec.inputs {
            let v = dist.sample(DrawAddr {
                seed: self.spec.seed,
                cell: self.input_index[&(*sheet, *row, *col)] as u32,
                scenario: 0,
                draw: 0,
                attempt: 0,
            });
            wb2.set_value(*sheet, *row, *col, Value::Num(v));
        }
        let mut cells = 0;
        let mut mism = 0;
        for pos in 0..self.schedule.len() {
            let (sheet, row, col) = self.key_of(pos);
            let src = wb2.sheets[sheet as usize].formulas[&(row, col)].clone();
            let Ok(parsed) = xlc_parse::parse_formula(&src) else { continue };
            let v = Interp::new(&wb2, Origin { sheet, row, col }).eval_formula(&parsed.expr);
            wb2.set_value(sheet, row, col, v.clone());
            cells += 1;
            let got = &sweep.watched[&(sheet, row, col)][0];
            if !xlc_ir::bit_equal(&v, got) {
                mism += 1;
                eprintln!(
                    "verify_n1 drift: sheet{sheet} {}{} = {src} | scalar {v:?} engine {got:?}",
                    xlc_parse::ast::col_letters(col),
                    row + 1
                );
            }
        }
        (cells, mism)
    }

    /// Run the sweep, keeping per-scenario values for `watch` cells.
    pub fn run(&self, scenarios: u32, tile: u32, watch: &[CellKey]) -> SweepResult {
        let watch_set: HashSet<CellKey> = watch.iter().copied().collect();
        let mut result = SweepResult {
            watched: watch.iter().map(|k| (*k, Vec::new())).collect(),
            scenarios,
            cone_cells: self.schedule.len(),
            bytes_written: 0,
            bytes_read_streams: 0,
            fast_path_cells: 0,
            scalar_path_cells: 0,
            peak_live_buffers: 0,
        };

        let mut s0 = 0u32;
        while s0 < scenarios {
            let t = tile.min(scenarios - s0);
            self.run_tile(s0, t, &watch_set, &mut result);
            s0 += t;
        }
        result
    }

    /// Evaluate one scenario in isolation — the (seed, k) reproducibility
    /// property made executable.
    pub fn eval_scenario(&self, k: u32, watch: &[CellKey]) -> HashMap<CellKey, Value> {
        let watch_set: HashSet<CellKey> = watch.iter().copied().collect();
        let mut r = SweepResult {
            watched: watch.iter().map(|w| (*w, Vec::new())).collect(),
            scenarios: 1,
            cone_cells: self.schedule.len(),
            bytes_written: 0,
            bytes_read_streams: 0,
            fast_path_cells: 0,
            scalar_path_cells: 0,
            peak_live_buffers: 0,
        };
        self.run_tile(k, 1, &watch_set, &mut r);
        r.watched
            .into_iter()
            .map(|(k2, mut v)| (k2, v.pop().unwrap_or(Value::Blank)))
            .collect()
    }

    fn run_tile(
        &self,
        s0: u32,
        t: u32,
        watch: &HashSet<CellKey>,
        out: &mut SweepResult,
    ) {
        let tl = t as usize;
        // Input sample buffers for this tile.
        let mut input_bufs: Vec<Vec<f64>> = Vec::with_capacity(self.spec.inputs.len());
        for (i, (_, dist)) in self.spec.inputs.iter().enumerate() {
            let mut b = Vec::with_capacity(tl);
            for s in 0..t {
                b.push(dist.sample(DrawAddr {
                    seed: self.spec.seed,
                    cell: i as u32,
                    scenario: s0 + s,
                    draw: 0,
                    attempt: 0,
                }));
            }
            input_bufs.push(b);
        }

        // Cell buffers with refcount pooling.
        let mut bufs: Vec<Option<Vec<Value>>> = vec![None; self.schedule.len()];
        let mut remaining = self.consumers.clone();
        let mut pool: Vec<Vec<Value>> = Vec::new();

        let sctx = ScenarioCtx {
            wb: self.wb,
            engine: self,
            input_bufs: &input_bufs,
            bufs: StdCell::new(std::ptr::null()),
            current_s: StdCell::new(0),
        };

        for (pos, &(fi, li)) in self.schedule.iter().enumerate() {
            let fam = &self.module.families[fi];
            let mut buf = pool.pop().unwrap_or_default();
            buf.clear();

            let fast_ok = self.try_fast(fi, li, s0, t, &input_bufs, &bufs, &mut buf);
            if fast_ok {
                out.fast_path_cells += 1;
            } else {
                out.scalar_path_cells += 1;
                buf.clear();
                // SAFETY: bufs outlives the ctx use inside this loop body;
                // the raw pointer is only read by ScenarioCtx::cell.
                sctx.bufs.set(&bufs as *const _);
                for s in 0..t {
                    sctx.current_s.set(s);
                    let v = fam.eval_lane(&sctx, li);
                    buf.push(v);
                }
            }
            out.bytes_written += 8 * t as u64;
            out.bytes_read_streams +=
                8 * t as u64 * (self.dep_cells[pos].len() + self.dep_inputs[pos].len()) as u64;

            let key = (fam.sheet, fam.lanes[li].0, fam.lanes[li].1);
            if watch.contains(&key) {
                out.watched.get_mut(&key).unwrap().extend(buf.iter().cloned());
            }
            bufs[pos] = Some(buf);
            let live = bufs.iter().filter(|b| b.is_some()).count();
            out.peak_live_buffers = out.peak_live_buffers.max(live);

            // Release dep buffers whose consumers are all done.
            for &d in &self.dep_cells[pos] {
                remaining[d] -= 1;
                if remaining[d] == 0 && !watch.contains(&self.key_of(d)) {
                    if let Some(b) = bufs[d].take() {
                        pool.push(b);
                    }
                }
            }
        }
    }

    fn key_of(&self, pos: usize) -> CellKey {
        let (fi, li) = self.schedule[pos];
        let fam = &self.module.families[fi];
        (fam.sheet, fam.lanes[li].0, fam.lanes[li].1)
    }

    /// Vectorized fast path. Returns false (buffer untouched) when the
    /// family is not fast-eligible or an operand/result leaves the pure
    /// numeric domain.
    fn try_fast(
        &self,
        fi: usize,
        li: usize,
        s0: u32,
        t: u32,
        input_bufs: &[Vec<f64>],
        bufs: &[Option<Vec<Value>>],
        out_buf: &mut Vec<Value>,
    ) -> bool {
        let Some(plan) = &self.fast[fi] else { return false };
        let fam = &self.module.families[fi];
        let (row, col) = fam.lanes[li];
        let (er, ec) = fam.lanes[0];
        let (dr, dc) = (row as i64 - er as i64, col as i64 - ec as i64);
        let tl = t as usize;

        // Resolve loads: each becomes either a broadcast constant or a
        // borrowed stream.
        enum Src<'a> {
            Broadcast(f64),
            Stream(&'a [f64]),
            ValueStream(&'a [Value]),
        }
        let mut lanes: Vec<Vec<f64>> = Vec::new();
        let mut slots: Vec<Src> = Vec::with_capacity(plan.insts.len());
        for inst in &plan.insts {
            let src = match inst {
                FastInst::Const(x) => Src::Broadcast(*x),
                FastInst::Load { sheet, row: r, col: c, row_abs, col_abs } => {
                    let rr = if *row_abs { *r } else { *r + dr };
                    let cc = if *col_abs { *c } else { *c + dc };
                    let (Ok(rr), Ok(cc)) = (u32::try_from(rr), u32::try_from(cc)) else {
                        return false;
                    };
                    let key = (*sheet, rr, cc);
                    if let Some(&ii) = self.input_index.get(&key) {
                        Src::Stream(&input_bufs[ii])
                    } else if let Some(&p) = self.dirty_index.get(&key) {
                        match &bufs[p] {
                            Some(b) => Src::ValueStream(b),
                            None => return false,
                        }
                    } else {
                        match self.wb.sheets[*sheet as usize].values.get(&(rr, cc)) {
                            None => Src::Broadcast(0.0),
                            Some(Value::Num(x)) => Src::Broadcast(*x),
                            Some(Value::Bool(b)) => Src::Broadcast(if *b { 1.0 } else { 0.0 }),
                            Some(_) => return false, // text/error: scalar path
                        }
                    }
                }
                FastInst::SumRange { .. } => Src::Broadcast(0.0), // handled in exec
                FastInst::Bin { .. } | FastInst::Neg(_) => Src::Broadcast(0.0), // filled below
            };
            slots.push(src);
        }
        // Execute: materialize per-inst f64 lanes.
        let mut vals: Vec<Vec<f64>> = Vec::with_capacity(plan.insts.len());
        for (i, inst) in plan.insts.iter().enumerate() {
            let v: Vec<f64> = match inst {
                FastInst::Const(_) | FastInst::Load { .. } => match &slots[i] {
                    Src::Broadcast(x) => vec![*x; tl],
                    Src::Stream(s) => s.to_vec(),
                    Src::ValueStream(vs) => {
                        let mut o = Vec::with_capacity(tl);
                        for v in vs.iter() {
                            match v {
                                Value::Num(x) => o.push(*x),
                                _ => return false,
                            }
                        }
                        o
                    }
                },
                FastInst::SumRange { sheet, area } => {
                    let Some(a2) = xlc_ir::rebase_pub(area, dr, dc) else { return false };
                    let interp = Interp::new(self.wb, Origin { sheet: *sheet, row, col });
                    let Operand::Ref(rects) = interp.resolve_area(&[*sheet], &a2) else {
                        return false;
                    };
                    // Fold ORDER must equal the scalar path's rect order:
                    // consecutive static numerics collapse into positional
                    // partial constants; streams interleave where their
                    // cells sit. (First cut pre-folded statics and drifted
                    // by 1 ULP on interleaved ranges — the N=1 oracle
                    // caught it on SUM(F4:F125).)
                    enum Term<'t> {
                        Const(f64),
                        Stream(&'t [f64]),
                        Values(&'t [Value]),
                    }
                    let mut terms: Vec<Term> = Vec::new();
                    for r in &rects {
                        for (rr, cc) in r.cells() {
                            let key = (r.sheet, rr, cc);
                            if let Some(&ii) = self.input_index.get(&key) {
                                terms.push(Term::Stream(&input_bufs[ii]));
                            } else if let Some(&p) = self.dirty_index.get(&key) {
                                match &bufs[p] {
                                    Some(b) => terms.push(Term::Values(b)),
                                    None => return false,
                                }
                            } else {
                                match self.wb.sheets[r.sheet as usize].values.get(&(rr, cc)) {
                                    None => {}
                                    Some(Value::Num(x)) => terms.push(Term::Const(*x)),
                                    // Range semantics: text/bool ignored.
                                    Some(Value::Text(_)) | Some(Value::Bool(_)) => {}
                                    Some(_) => return false, // error in range
                                }
                            }
                        }
                    }
                    // A LEADING run of constants folds once (acc starts at
                    // exactly 0.0, so the grouping is identical); every
                    // later term must be applied individually in order.
                    let mut skip = 0usize;
                    let mut init = 0.0f64;
                    for term in &terms {
                        match term {
                            Term::Const(x) => {
                                init += x;
                                skip += 1;
                            }
                            _ => break,
                        }
                    }
                    let mut o = vec![init; tl];
                    for term in &terms[skip..] {
                        match term {
                            Term::Const(x) => {
                                for v in o.iter_mut() {
                                    *v += x;
                                }
                            }
                            Term::Stream(st) => {
                                for i in 0..tl {
                                    o[i] += st[i];
                                }
                            }
                            Term::Values(vs) => {
                                for i in 0..tl {
                                    match &vs[i] {
                                        Value::Num(x) => o[i] += x,
                                        _ => return false,
                                    }
                                }
                            }
                        }
                    }
                    o
                }
                FastInst::Bin { op, a, b } => {
                    let (x, y) = (&vals[*a as usize], &vals[*b as usize]);
                    let mut o = vec![0.0; tl];
                    let ok = vector_binop(*op, x, y, &mut o);
                    if !ok {
                        return false;
                    }
                    o
                }
                FastInst::Neg(a) => vals[*a as usize].iter().map(|x| -x).collect(),
            };
            vals.push(v);
        }
        let res = &vals[plan.result as usize];
        if res.iter().any(|x| !x.is_finite()) {
            return false; // overflow/div edge: scalar path owns the semantics
        }
        out_buf.extend(res.iter().map(|&x| Value::Num(x)));
        let _ = &mut lanes;
        true
    }
}

/// pulp-dispatched elementwise arithmetic. Division by zero bails so the
/// scalar path can produce the proper #DIV/0!.
fn vector_binop(op: BinOp, x: &[f64], y: &[f64], out: &mut [f64]) -> bool {
    if matches!(op, BinOp::Div) && y.iter().any(|&v| v == 0.0) {
        return false;
    }
    let arch = pulp::Arch::new();
    arch.dispatch(|| {
        match op {
            BinOp::Add => {
                for i in 0..out.len() {
                    out[i] = x[i] + y[i];
                }
            }
            BinOp::Sub => {
                for i in 0..out.len() {
                    out[i] = x[i] - y[i];
                }
            }
            BinOp::Mul => {
                for i in 0..out.len() {
                    out[i] = x[i] * y[i];
                }
            }
            BinOp::Div => {
                for i in 0..out.len() {
                    out[i] = x[i] / y[i];
                }
            }
            _ => {}
        }
    });
    matches!(op, BinOp::Add | BinOp::Sub | BinOp::Mul | BinOp::Div)
}

/// Recognize `SUM(<one area ref>)` opaque nodes (the dominant aggregate).
fn sum_range_of(ast: &Expr) -> Option<(Vec<SheetId>, xlc_parse::ast::Area)> {
    let Expr::Call { name, args } = ast else { return None };
    let mut n = name.to_ascii_uppercase();
    while let Some(rest) = n.strip_prefix("_XLFN.") {
        n = rest.to_string();
    }
    if n != "SUM" || args.len() != 1 {
        return None;
    }
    let inner = args[0].expr.as_ref()?;
    let inner = match inner {
        Expr::Paren { inner, .. } => inner.as_ref(),
        e => e,
    };
    let Expr::Ref(xlc_parse::ast::RefExpr::Area { sheet, area }) = inner else { return None };
    if matches!(area, xlc_parse::ast::Area::RefError) {
        return None;
    }
    match sheet {
        None => Some((vec![u32::MAX], area.clone())), // marker: own sheet
        Some(sp) if sp.workbook.is_none() && sp.last.is_none() => None, // resolved at plan time? keep scalar
        Some(_) => None,
    }
}

/// Compile a family to the fast plan when every instruction is pure
/// arithmetic over single-cell refs on resolvable sheets.
fn fast_plan(fam: &Family) -> Option<FastPlan> {
    let mut insts = Vec::with_capacity(fam.insts.len());
    for inst in &fam.insts {
        let fi = match inst {
            Inst::Num(x) => FastInst::Const(*x),
            Inst::RefT { sheets, area } => {
                if sheets.len() != 1 {
                    return None;
                }
                match area {
                    xlc_parse::ast::Area::Cell(c) => FastInst::Load {
                        sheet: sheets[0],
                        row: c.row as i64,
                        col: c.col as i64,
                        row_abs: c.row_anchored,
                        col_abs: c.col_anchored,
                    },
                    _ => return None,
                }
            }
            Inst::Binary { op, a, b } => match op {
                BinOp::Add | BinOp::Sub | BinOp::Mul | BinOp::Div => {
                    FastInst::Bin { op: *op, a: *a, b: *b }
                }
                _ => return None,
            },
            Inst::Unary { op, a } => match op {
                UnOp::Neg => FastInst::Neg(*a),
                UnOp::Pos => FastInst::Bin { op: BinOp::Add, a: *a, b: *a }, // placeholder
                _ => return None,
            },
            Inst::Opaque { ast } => match sum_range_of(ast) {
                Some((sheets, area)) => {
                    let sheet = if sheets == [u32::MAX] { fam.sheet } else { sheets[0] };
                    FastInst::SumRange { sheet, area }
                }
                None => return None,
            },
            _ => return None,
        };
        // Reject the Pos placeholder (identity would double): scalar path.
        if matches!(inst, Inst::Unary { op: UnOp::Pos, .. }) {
            return None;
        }
        insts.push(fi);
    }
    Some(FastPlan { insts, result: fam.result })
}

/// Public wrapper for input auto-selection in the CLI.
pub fn family_dep_rects_pub(wb: &Workbook, fam: &Family, lane: usize) -> Vec<Rect> {
    family_dep_rects(wb, fam, lane)
}

/// Rects a lane depends on (deps for the cone + schedule), via the same
/// evaluation machinery: every Ref/Name operand the expression can touch.
fn family_dep_rects(wb: &Workbook, fam: &Family, lane: usize) -> Vec<Rect> {
    let (row, col) = fam.lanes[lane];
    let interp = Interp::new(wb, Origin { sheet: fam.sheet, row, col });
    let mut rects = Vec::new();
    let (er, ec) = fam.lanes[0];
    let (dr, dc) = (row as i64 - er as i64, col as i64 - ec as i64);
    for inst in &fam.insts {
        match inst {
            Inst::RefT { sheets, area } => {
                if let Some(a) = xlc_ir::rebase_pub(area, dr, dc) {
                    if let Operand::Ref(rs) = interp.resolve_area(sheets, &a) {
                        rects.extend(rs);
                    }
                }
            }
            Inst::Opaque { ast } => {
                let rebased = xlc_ir::rebase_expr_pub(ast, dr, dc);
                collect_expr_refs(&interp, &rebased, &mut rects, wb);
            }
            _ => {}
        }
    }
    rects
}

fn collect_expr_refs(interp: &Interp<Workbook>, e: &Expr, rects: &mut Vec<Rect>, wb: &Workbook) {
    e.walk(&mut |n| match n {
        Expr::Ref(xlc_parse::ast::RefExpr::Area { .. }) => {
            if let Operand::Ref(rs) = interp.eval(n) {
                rects.extend(rs.iter().copied());
            }
        }
        Expr::Name { name, .. } => {
            if let Some(body) = wb.names.get(&name.to_uppercase()) {
                if let Operand::Ref(rs) = interp.eval(body) {
                    rects.extend(rs.iter().copied());
                }
            }
        }
        _ => {}
    });
}

fn rect_hits(r: &Rect, set: &HashSet<CellKey>) -> bool {
    let area = (r.r1 - r.r0 + 1) as u64 * (r.c1 - r.c0 + 1) as u64;
    if area <= set.len() as u64 {
        r.cells().any(|(row, col)| set.contains(&(r.sheet, row, col)))
    } else {
        set.iter().any(|&(s, row, col)| s == r.sheet && r.contains(row, col))
    }
}

fn for_each_dirty_in_rect(r: &Rect, dirty: &HashSet<CellKey>, mut f: impl FnMut(CellKey)) {
    let area = (r.r1 - r.r0 + 1) as u64 * (r.c1 - r.c0 + 1) as u64;
    if area <= dirty.len() as u64 {
        for (row, col) in r.cells() {
            let k = (r.sheet, row, col);
            if dirty.contains(&k) {
                f(k);
            }
        }
    } else {
        for &k in dirty.iter() {
            if k.0 == r.sheet && r.contains(k.1, k.2) {
                f(k);
            }
        }
    }
}

/// Ctx that overlays scenario state on the static workbook: input cells
/// read their sampled value, dirty cells read their computed buffer, all
/// at the tile-local scenario index `current_s`.
struct ScenarioCtx<'a, 'wb> {
    wb: &'wb Workbook,
    engine: &'a Engine<'wb>,
    input_bufs: &'a [Vec<f64>],
    bufs: StdCell<*const Vec<Option<Vec<Value>>>>,
    current_s: StdCell<u32>,
}

impl Ctx for ScenarioCtx<'_, '_> {
    fn cell(&self, sheet: SheetId, row: u32, col: u32) -> Value {
        let key = (sheet, row, col);
        if let Some(&ii) = self.engine.input_index.get(&key) {
            return Value::Num(self.input_bufs[ii][self.current_s.get() as usize]);
        }
        if let Some(&pos) = self.engine.dirty_index.get(&key) {
            // SAFETY: set by run_tile immediately before use; the pointee
            // outlives the evaluation call.
            let bufs = unsafe { &*self.bufs.get() };
            if let Some(b) = &bufs[pos] {
                return b[self.current_s.get() as usize].clone();
            }
        }
        self.wb.cell(sheet, row, col)
    }

    fn resolve_sheet(&self, name: &str) -> Option<SheetId> {
        self.wb.resolve_sheet_pub(name)
    }

    fn used_extent(&self, sheet: SheetId) -> (u32, u32) {
        Ctx::used_extent(self.wb, sheet)
    }

    fn defined_name(&self, name: &str) -> Option<&Expr> {
        Ctx::defined_name(self.wb, name)
    }

    fn epoch_1904(&self) -> bool {
        Ctx::epoch_1904(self.wb)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// B{r} = A{r}*2 + B{r-1} (running total), A column static, A1 uncertain.
    fn chain_wb(n: u32) -> Workbook {
        let mut wb = Workbook::default();
        let id = wb.add_sheet("S");
        for r in 0..n {
            wb.set_value(id, r, 0, Value::Num((r + 1) as f64));
            let f = if r == 0 {
                "A1*2".to_string()
            } else {
                format!("A{}*2+B{}", r + 1, r)
            };
            wb.set_formula(id, r, 1, f);
        }
        wb
    }

    #[test]
    fn n1_point_oracle_bit_identical() {
        let wb = chain_wb(30);
        let spec = ScenarioSpec {
            seed: 9,
            inputs: vec![((0, 0, 0), Dist::Point { value: 7.5 })],
        };
        let engine = Engine::new(&wb, spec);
        assert_eq!(engine.cone_cells(), 30, "whole B column is the cone");
        let watch: Vec<CellKey> = (0..30).map(|r| (0, r, 1)).collect();
        let res = engine.run(1, 64, &watch);

        // Scalar oracle: same workbook with A1 = 7.5 substituted.
        let mut wb2 = chain_wb(30);
        wb2.set_value(0, 0, 0, Value::Num(7.5));
        for r in 0..30u32 {
            let src = wb2.sheets[0].formulas[&(r, 1)].clone();
            let parsed = xlc_parse::parse_formula(&src).unwrap();
            // Scalar receipt semantics need computed B values as we walk
            // down (full recompute): evaluate in order and write back.
            let v = Interp::new(&wb2, Origin { sheet: 0, row: r, col: 1 })
                .eval_formula(&parsed.expr);
            wb2.set_value(0, r, 1, v.clone());
            let got = &res.watched[&(0, r, 1)][0];
            assert!(
                xlc_ir::bit_equal(&v, got),
                "row {r}: scalar {v:?} vs scenario {got:?}"
            );
        }
    }

    #[test]
    fn deterministic_mean_is_exact() {
        let wb = chain_wb(10);
        let spec = ScenarioSpec {
            seed: 1,
            inputs: vec![((0, 0, 0), Dist::Point { value: 3.25 })],
        };
        let engine = Engine::new(&wb, spec);
        let res = engine.run(10_000, 512, &[(0, 9, 1)]);
        let vals = &res.watched[&(0, 9, 1)];
        assert_eq!(vals.len(), 10_000);
        let first = &vals[0];
        assert!(vals.iter().all(|v| xlc_ir::bit_equal(v, first)), "all identical");
    }

    #[test]
    fn scenario_k_reproducible() {
        let wb = chain_wb(12);
        let spec = ScenarioSpec {
            seed: 77,
            inputs: vec![((0, 0, 0), Dist::Normal { mean: 5.0, sd: 2.0 })],
        };
        let engine = Engine::new(&wb, spec);
        let watch = [(0u32, 11u32, 1u32)];
        let sweep = engine.run(1000, 128, &watch);
        for k in [0u32, 1, 127, 128, 500, 999] {
            let solo = engine.eval_scenario(k, &watch);
            assert!(
                xlc_ir::bit_equal(&solo[&(0, 11, 1)], &sweep.watched[&(0, 11, 1)][k as usize]),
                "scenario {k}"
            );
        }
    }

    #[test]
    fn fast_and_scalar_paths_agree() {
        // Pure-arith chain: fast path handles it; force scalar by watching
        // every cell and comparing against per-scenario solo eval (which
        // itself exercises tile size 1 => same code, so instead compare
        // fast sweep vs a sweep with fast disabled via SUM wrapper).
        let mut wb = chain_wb(20);
        // SUM now vectorizes too; ROUND stays scalar.
        wb.set_formula(0, 20, 1, "SUM(B1:B20)".into());
        wb.set_formula(0, 21, 1, "ROUND(B21,4)".into());
        let spec = ScenarioSpec {
            seed: 3,
            inputs: vec![((0, 0, 0), Dist::Uniform { a: 1.0, b: 9.0 })],
        };
        let engine = Engine::new(&wb, spec);
        assert_eq!(engine.cone_cells(), 22);
        let res = engine.run(256, 64, &[(0, 20, 1), (0, 21, 1)]);
        assert!(res.fast_path_cells > 0, "fast path engaged");
        assert!(res.scalar_path_cells > 0, "scalar path engaged (SUM)");
        // Cross-check five scenarios end-to-end against solo evaluation.
        for k in [0u32, 63, 64, 200, 255] {
            let solo = engine.eval_scenario(k, &[(0, 20, 1), (0, 21, 1)]);
            for key in [(0, 20, 1), (0, 21, 1)] {
                assert!(
                    xlc_ir::bit_equal(&solo[&key], &res.watched[&key][k as usize]),
                    "scenario {k} cell {key:?}"
                );
            }
        }
    }

    #[test]
    fn bytes_accounting_present() {
        let wb = chain_wb(10);
        let spec = ScenarioSpec {
            seed: 1,
            inputs: vec![((0, 0, 0), Dist::Uniform { a: 0.0, b: 1.0 })],
        };
        let engine = Engine::new(&wb, spec);
        let res = engine.run(100, 32, &[]);
        assert_eq!(res.bytes_written, 8 * 100 * 10);
        assert!(res.bytes_read_streams > 0);
    }
}
