/**
 * Example: AI creates a todo app at runtime.
 *
 * This shows what the AI would write to PluresDB to create a functioning
 * todo application — no code generation, no compilation.
 *
 * The AI conversation might go:
 *   User: "Make me a todo app"
 *   AI: *writes this data to PluresDB*
 *   *App appears on screen immediately*
 */

import type { CanvasDocument } from './format.js';

export const todoAppCanvas: CanvasDocument = {
  version: '1.0.0',
  meta: {
    title: 'Todo App',
    description: 'Simple todo list — created by AI at runtime',
    author: 'ai:cerebellum',
    createdAt: '2026-05-08T10:00:00Z',
    modifiedAt: '2026-05-08T10:00:00Z',
    tags: ['productivity', 'todo', 'example'],
    id: 'canvas:todo-app-001',
  },
  tree: {
    id: 'root',
    type: 'PluginContentArea',
    props: { title: 'My Todos' },
    children: [
      {
        id: 'status-bar',
        type: 'StatusBar',
        bindings: {
          items: {
            key: 'todo:status-items',
            readTransform: 'identity',
          },
        },
      },
      {
        id: 'todo-settings',
        type: 'SettingsPanel',
        props: {
          groupName: 'New Todo',
          settings: [
            {
              key: 'todo:new-text',
              type: 'text',
              label: 'What needs to be done?',
              default: '',
            },
          ],
        },
        bindings: {
          getValue: { key: 'todo:settings-getter' },
          setValue: { key: 'todo:settings-setter' },
        },
      },
      {
        id: 'add-button',
        type: 'Button',
        props: { label: 'Add Todo', variant: 'primary' },
        // Visibility: only show when input has text
        visible: { key: 'todo:new-text', op: 'truthy' },
      },
      {
        id: 'clear-button',
        type: 'Button',
        props: { label: 'Clear Completed', variant: 'secondary' },
        visible: { key: 'todo:has-completed', op: 'truthy' },
      },
      {
        id: 'confirm-clear',
        type: 'Dialog',
        bindings: {
          open: { key: 'todo:confirm-dialog-open' },
        },
        props: {
          title: 'Clear completed?',
          message: 'This will remove all completed todos.',
          confirmLabel: 'Clear',
          cancelLabel: 'Keep',
        },
      },
    ],
  },
  data: {
    'todo:items': [],
    'todo:new-text': '',
    'todo:filter': 'all', // all | active | completed
    'todo:confirm-dialog-open': false,
    'todo:has-completed': false,
    'todo:status-items': [
      { id: 'count', label: '0 items', position: 'left' },
      { id: 'filter', label: 'All', position: 'right' },
    ],
  },
  rules: [
    {
      id: 'no-empty-todo',
      description: 'Cannot add an empty todo',
      when: { key: 'todo:new-text', op: 'falsy' },
      action: 'gate',
      message: 'Todo text cannot be empty',
      severity: 'error',
    },
    {
      id: 'no-duplicate-todo',
      description: 'Cannot add duplicate todo text',
      when: 'todo:adding',
      require: { key: 'todo:is-duplicate', op: 'falsy' },
      action: 'block',
      message: 'This todo already exists',
      severity: 'warning',
    },
  ],
  procedures: [
    {
      id: 'add-todo',
      description: 'Add a new todo item',
      trigger: { kind: 'on_click', nodeId: 'add-button' },
      steps: [
        {
          kind: 'append',
          key: 'todo:items',
          value: { text: '${todo:new-text}', done: false, id: '${_timestamp}' },
        },
        { kind: 'set', key: 'todo:new-text', value: '' },
        { kind: 'emit', value: 'todo:updated' },
      ],
    },
    {
      id: 'open-clear-dialog',
      description: 'Confirm before clearing completed',
      trigger: { kind: 'on_click', nodeId: 'clear-button' },
      steps: [{ kind: 'set', key: 'todo:confirm-dialog-open', value: true }],
    },
    {
      id: 'confirm-clear',
      description: 'Actually clear completed items',
      trigger: { kind: 'on_event', event: 'dialog:confirm' },
      steps: [
        // Would need a 'filter' step type — showing the pattern
        { kind: 'set', key: 'todo:confirm-dialog-open', value: false },
        { kind: 'emit', value: 'todo:updated' },
      ],
    },
  ],
  schema: [
    { key: 'todo:items', type: 'Array<{ text: string; done: boolean; id: string }>', description: 'All todo items' },
    { key: 'todo:new-text', type: 'string', description: 'Current input text', default: '' },
    { key: 'todo:filter', type: "'all' | 'active' | 'completed'", description: 'Active filter', default: 'all' },
    { key: 'todo:confirm-dialog-open', type: 'boolean', description: 'Confirmation dialog state', default: false },
    { key: 'todo:has-completed', type: 'boolean', description: 'Whether any items are complete', default: false },
    { key: 'todo:status-items', type: 'StatusItem[]', description: 'Status bar items' },
  ],
};
