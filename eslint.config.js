import eslint from '@eslint/js';
import tseslint from 'typescript-eslint';
import sveltePlugin from 'eslint-plugin-svelte';
import svelteParser from 'svelte-eslint-parser';
import designDojoPlugin from '@plures/design-dojo/enforce';

export default tseslint.config(
  eslint.configs.recommended,
  ...tseslint.configs.recommended,
  {
    ignores: ['dist/', 'node_modules/', '.svelte-kit/', 'build/'],
  },
  // Apply design-dojo recommended config (no-local-components + prefer-design-dojo-imports)
  designDojoPlugin.configs.recommended,
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
    },
  },
);
