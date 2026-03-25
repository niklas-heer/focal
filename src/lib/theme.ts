import { EditorView } from '@codemirror/view'
import { Extension } from '@codemirror/state'
import { HighlightStyle, syntaxHighlighting } from '@codemirror/language'
import { tags as t } from '@lezer/highlight'

export const tokyoNightTheme: Extension = EditorView.theme(
  {
    '&': {
      color: 'var(--tn-fg)',
      backgroundColor: 'transparent',
    },
    '.cm-content': {
      caretColor: 'var(--tn-cursor)',
      fontFamily: 'var(--font-prose)',
    },
    '.cm-cursor': {
      borderLeftColor: 'var(--tn-cursor)',
      borderLeftWidth: '2px',
    },
    '.cm-selectionBackground': {
      backgroundColor: 'var(--tn-selection)',
    },
    '&.cm-focused .cm-selectionBackground': {
      backgroundColor: 'var(--tn-selection)',
    },
    '.cm-panels': {
      backgroundColor: 'var(--tn-bg-dark)',
      color: 'var(--tn-fg)',
    },
    '.cm-activeLine': {
      backgroundColor: 'transparent',
    },
    '.cm-gutters': {
      display: 'none',
    },
  },
  { dark: true }
)

export const tokyoNightHighlight = syntaxHighlighting(
  HighlightStyle.define([
    { tag: t.keyword,                                                                      color: '#a08898' },
    { tag: [t.name, t.deleted, t.character, t.propertyName, t.macroName],                 color: '#b4b0a8' },
    { tag: [t.function(t.variableName), t.labelName],                                     color: '#7aa8b8' },
    { tag: [t.color, t.constant(t.name), t.standard(t.name)],                             color: '#b8906a' },
    { tag: [t.definition(t.name), t.separator],                                           color: '#b4b0a8' },
    { tag: [t.typeName, t.className, t.number, t.changed, t.annotation, t.modifier,
             t.self, t.namespace],                                                         color: '#b0a070' },
    { tag: [t.operator, t.operatorKeyword, t.escape, t.regexp, t.special(t.string)],      color: '#78b8c8' },
    { tag: [t.url, t.link],                                                                color: '#4eb8c8', textDecoration: 'underline' },
    { tag: [t.meta, t.comment],                                                            color: '#6a6560', fontStyle: 'italic' },
    { tag: t.strong,                                                                       fontWeight: 'bold' },
    { tag: t.emphasis,                                                                     fontStyle: 'italic' },
    { tag: t.strikethrough,                                                                textDecoration: 'line-through' },
    { tag: t.heading,                                                                      fontWeight: 'bold', color: '#b4b0a8' },
    { tag: [t.atom, t.bool, t.special(t.variableName)],                                   color: '#b8906a' },
    { tag: [t.processingInstruction, t.string, t.inserted],                               color: '#8aaa6a' },
    { tag: t.invalid,                                                                      color: '#c07070' },
  ])
)
