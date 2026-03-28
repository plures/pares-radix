import eslint from '@eslint/js';
import tseslint from 'typescript-eslint';
import sveltePlugin from 'eslint-plugin-svelte';
import svelteParser from 'svelte-eslint-parser';
import designDojoPlugin from '@plures/eslint-plugin-design-dojo';

export default tseslint.config(
  eslint.configs.recommended,
  ...tseslint.configs.recommended,
  {
    ignores: ['dist/', 'node_modules/', '.svelte-kit/', 'build/'],
  },
  {
    files: ['**/*.svelte'],
    plugins: {
      svelte: sveltePlugin,
      'design-dojo': designDojoPlugin,
    },
    languageOptions: {
      parser: svelteParser,
      parserOptions: {
        parser: tseslint.parser,
      },
    },
    rules: {
      ...sveltePlugin.configs.recommended.rules,
      'design-dojo/no-local-primitives': 'error',
      'design-dojo/prefer-design-dojo-imports': 'warn',
    },
  },
  {
    files: ['**/*.ts', '**/*.js'],
    plugins: {
      'design-dojo': designDojoPlugin,
    },
    rules: {
      'design-dojo/prefer-design-dojo-imports': 'warn',
    },
  },
);
