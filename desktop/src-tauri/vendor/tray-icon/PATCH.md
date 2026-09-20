Vendored `tray-icon` 0.24.2 with macOS fixes:

1. Show the status-item menu via `popUpStatusItemMenu` (upstream PR 318).
2. Do not unwrap `NSEvent.window` on click — that panic aborted Dictator before the menu could open.
3. Icon-only `NSStatusItem` (`ImageOnly`, square length, size before `setImage`) so the menu bar does not draw a second mic slot.
