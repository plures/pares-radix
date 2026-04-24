import eslint from '@eslint/js';
import tseslint from 'typescript-eslint';
import sveltePlugin from 'eslint-plugin-svelte';
import svelteParser from 'svelte-eslint-parser';
import globals from 'globals';

export default tseslint.config(
  eslint.configs.recommended,
  ...tseslint.configs.recommended,
  {
    ignores: ['dist/', 'node_modules/', '.svelte-kit/', 'build/'],
  },
  // Global language options for browser + node environments
  {
    languageOptions: {
      globals: {
        ...globals.browser,
        ...globals.node,
      },
    },
    rules: {
      // TypeScript handles undefined-variable checks better than ESLint's no-undef.
      // Disable it globally to avoid false positives on DOM types and TS declarations.
      'no-undef': 'off',
      // Allow _-prefixed parameters/variables to mark intentionally unused args.
      '@typescript-eslint/no-unused-vars': [
        'error',
        { argsIgnorePattern: '^_', varsIgnorePattern: '^_', caughtErrorsIgnorePattern: '^_' },
      ],
    },
  },
  {
    files: ['**/*.svelte'],
    plugins: {
      svelte: sveltePlugin,
    },
    languageOptions: {
      parser: svelteParser,
      parserOptions: {
        parser: tseslint.parser,
      },
    },
    rules: {
      ...sveltePlugin.configs.recommended.rules,
      // {@html} is used only with pre-escaped content (renderMarkdown) or
      // trusted constant strings (tuiCssOverrides).  Both are safe.
      'svelte/no-at-html-tags': 'off',
    },
  },
);
