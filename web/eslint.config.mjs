// Google TypeScript Style, via gts, plus React rules.
// gts owns formatting and the TypeScript rules. We add only React-specific ones.
// Import path note: gts 7.0.0's root `eslint.config.js` requires './src/index.js',
// which it does not publish. The copy under build/ resolves correctly.
import gts from 'gts/build/eslint.config.js';
import reactHooks from 'eslint-plugin-react-hooks';
import reactRefresh from 'eslint-plugin-react-refresh';

export default [
  ...gts,
  {
    ignores: [
      'dist/**',
      'node_modules/**',
      // Generated: ts-rs wire types and wasm-pack output (07-wasm-testing.md).
      'src/api/generated/**',
      'src/engine/generated/**',
    ],
  },
  {
    // gts points the TypeScript parser at a tsconfig it does not publish, so we
    // repoint it. `projectService` lets typescript-eslint find our tsconfig.
    files: ['**/*.{ts,tsx,mts,cts}'],
    languageOptions: {
      parserOptions: {
        project: ['./tsconfig.json'],
        tsconfigRootDir: import.meta.dirname,
      },
    },
  },
  {
    files: ['**/*.{ts,tsx}'],
    plugins: {
      'react-hooks': reactHooks,
      'react-refresh': reactRefresh,
    },
    rules: {
      ...reactHooks.configs.recommended.rules,
      'react-refresh/only-export-components': [
        'warn',
        {allowConstantExport: true},
      ],
    },
  },
];
