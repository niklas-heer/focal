+++
schema_version = 1
id = "01M3N1CC685R2SEASNER5DE1ZT"
title = "Build Focal as a native AppKit application"
date = "2026-09-28"
status = "superseded"
tags = ["architecture", "platform"]
supersedes = []
superseded_by = []
depends_on = []
related_to = []
+++
## Decision

Build Focal as a native macOS application in Swift with AppKit, replacing the Electron, React and CodeMirror prototype. SwiftUI may be used only for peripheral UI such as settings; the editor, its text rendering and the window chrome are AppKit. The only web view is the isolated, lazily created Mermaid renderer.

## Context

The first Focal (March–April 2026) was built with Electron, React, TypeScript and CodeMirror 6. It stalled on the core problem: its leftover smoke tests (`scripts/smoke-scroll-render.js`, `.tmp-*` debugging scripts) chase Markdown decorations that disappeared while scrolling, and its last screenshot still shows raw `**`, `#` and fence markers. It never reached Bear-style live rendering. That code is preserved at commit `a72a464`.

Niklas wants a Mac-native editor that matches iA Writer's typography and Bear's live rendering. AppKit's `NSTextView` and TextKit give direct control over glyph layout, hidden characters, embedded views, input methods, spell checking and accessibility, which is what live Markdown rendering needs. SwiftUI's text editing is too limited for this. Keywink already proves the native toolchain here: xcodebuild behind mise, Developer ID signing and notarization.

Alternatives considered:

- Keep Electron and fix the rendering: the web approach already failed once, and the result would still not feel native.
- A native shell around a web editor (CodeMirror or ProseMirror in `WKWebView`): Mermaid and math come for free, but editing, scrolling and typography still depend on the web stack.

## Consequences

- The editor feels and behaves like a Mac app, with system text services, dictation, spell checking, VoiceOver and native tabs.
- Mermaid is the one feature that needs a browser engine. It runs in a hidden `WKWebView` created only when a document contains a Mermaid block.
- Math has no free web renderer, so it needs a native typesetter (SwiftMath is the candidate).
- The app runs on macOS only, and CI needs macOS runners.
