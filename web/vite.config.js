import { defineConfig } from 'vite';

// Relative base: the built site works at any mount point (github.io/xlc/,
// a custom domain root, Netlify, file://).
export default defineConfig({
  base: './',
});
