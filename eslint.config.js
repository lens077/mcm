import js from "@eslint/js";
import globals from "globals";
import reactHooks from "eslint-plugin-react-hooks";
import reactRefresh from "eslint-plugin-react-refresh";
import tseslint from "typescript-eslint";

export default tseslint.config(
  { ignores: ["dist", "build", "coverage", "node_modules", "target", "src-tauri/target"] },
  js.configs.recommended,
  ...tseslint.configs.recommended,

  // Browser code: the app itself.
  {
    files: ["src/**/*.{ts,tsx}"],
    languageOptions: {
      ecmaVersion: 2022,
      globals: globals.browser,
    },
    plugins: { "react-hooks": reactHooks, "react-refresh": reactRefresh },
    rules: {
      ...reactHooks.configs.recommended.rules,
      "react-refresh/only-export-components": ["warn", { allowConstantExport: true }],
    },
  },

  // Node scripts and build config run outside the browser.
  {
    files: ["scripts/**/*.{js,mjs}", "*.config.{ts,js}", "eslint.config.js"],
    languageOptions: {
      ecmaVersion: 2022,
      globals: globals.node,
    },
  },

  // The WebDriver suite has both Node and injected WebdriverIO globals.
  {
    files: ["tests/e2e/**/*.ts"],
    languageOptions: {
      ecmaVersion: 2022,
      globals: {
        ...globals.node,
        ...globals.mocha,
        $: "readonly",
        browser: "readonly",
        expect: "readonly",
      },
    },
    rules: {
      // WebdriverIO's ambient types are not installed in this repo.
      "@typescript-eslint/no-explicit-any": "off",
    },
  },
);
