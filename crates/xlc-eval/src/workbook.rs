//! In-memory workbook model implementing `Ctx`. Pure data — calamine
//! ingest lives in xlc-cli; this crate never does I/O.

use std::collections::{HashMap, HashSet};

use crate::interp::{Ctx, SheetId};
use crate::value::Value;
use xlc_parse::ast::Expr;

#[derive(Default)]
pub struct Sheet {
    pub name: String,
    /// (row, col) → cached value (Excel's last computed value).
    pub values: HashMap<(u32, u32), Value>,
    /// (row, col) → formula source text.
    pub formulas: HashMap<(u32, u32), String>,
    /// Highest used (row, col).
    pub extent: (u32, u32),
    /// Cells whose formula is a legacy CSE array formula (`<f t="array">`).
    /// Scalar evaluation is wrong for these; they are excluded until the
    /// IR provides array semantics (§8.5).
    pub array_cells: HashSet<(u32, u32)>,
}

#[derive(Default)]
pub struct Workbook {
    pub sheets: Vec<Sheet>,
    by_name: HashMap<String, SheetId>,
    /// Defined names → parsed body (only refs/constants matter early).
    pub names: HashMap<String, Expr>,
    /// Defined names whose body references another workbook — cells using
    /// them are external-ref exclusions (Law 9).
    pub external_names: HashSet<String>,
    pub epoch_1904: bool,
}

impl Workbook {
    /// Public sheet-name resolution (case-insensitive) for lowering.
    pub fn resolve_sheet_pub(&self, name: &str) -> Option<SheetId> {
        self.by_name.get(&name.to_lowercase()).copied()
    }

    pub fn add_sheet(&mut self, name: &str) -> SheetId {
        let id = self.sheets.len() as SheetId;
        self.sheets.push(Sheet { name: name.to_string(), ..Default::default() });
        // Sheet-name lookup is case-insensitive in Excel.
        self.by_name.insert(name.to_lowercase(), id);
        id
    }

    pub fn set_value(&mut self, sheet: SheetId, row: u32, col: u32, v: Value) {
        let s = &mut self.sheets[sheet as usize];
        s.extent = (s.extent.0.max(row), s.extent.1.max(col));
        s.values.insert((row, col), v);
    }

    pub fn set_formula(&mut self, sheet: SheetId, row: u32, col: u32, src: String) {
        let s = &mut self.sheets[sheet as usize];
        s.extent = (s.extent.0.max(row), s.extent.1.max(col));
        s.formulas.insert((row, col), src);
    }
}

impl Ctx for Workbook {
    fn cell(&self, sheet: SheetId, row: u32, col: u32) -> Value {
        self.sheets
            .get(sheet as usize)
            .and_then(|s| s.values.get(&(row, col)).cloned())
            .unwrap_or(Value::Blank)
    }

    fn resolve_sheet(&self, name: &str) -> Option<SheetId> {
        self.by_name.get(&name.to_lowercase()).copied()
    }

    fn used_extent(&self, sheet: SheetId) -> (u32, u32) {
        self.sheets.get(sheet as usize).map(|s| s.extent).unwrap_or((0, 0))
    }

    fn defined_name(&self, name: &str) -> Option<&Expr> {
        self.names.get(&name.to_uppercase()).or_else(|| self.names.get(name))
    }

    fn epoch_1904(&self) -> bool {
        self.epoch_1904
    }
}
