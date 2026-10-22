// ESLint for the web version: `npm run lint` in web/.
import js from "@eslint/js";
import globals from "globals";

export default [
  js.configs.recommended,
  {
    languageOptions: {
      ecmaVersion: "latest",
      sourceType: "module",
      globals: globals.browser,
    },
    rules: {
      "no-unused-vars": ["error", { args: "after-used", argsIgnorePattern: "^_" }],
      eqeqeq: ["error", "always", { null: "ignore" }],
      "no-var": "error",
      "prefer-const": "error",
      "no-implicit-globals": "error",
      "no-shadow": "error",
      "no-return-assign": ["error", "except-parens"],
    },
  },
  {
    // The engine's worker and the offline service worker.
    files: ["worker.js", "sw.js"],
    languageOptions: { globals: { ...globals.worker, ...globals.serviceworker } },
  },
  {
    files: ["eslint.config.js"],
    languageOptions: { globals: globals.node },
  },
];
