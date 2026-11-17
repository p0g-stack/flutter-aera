# Draft PR body: desktop integration for Host API 2 (bit 12)

**Patch:** `0001-plugin_api-let-a-desktop-plugin-use-Files-and-the-ke.patch`,
on `aera-16.0` at 1cabc509 (standalone).

**Title:** plugin_api: let a desktop plugin use Files and the keyboard over its desktop

**What:** A Host API 2 worker with the optional desktop cannot ask for a
file while the desktop is open (`PICK_FILE` is refused). It cannot show
or hide AERA's on-screen keyboard for a focused text field. It does not
learn how much of its frame the keyboard covers. Keys without an ASCII
keysym (`é`, `€`, `π`) are dropped. This patch adds one optional feature
bit for these four gaps. Protocol version 2 and the packet size stay the
same, and no existing number changes.

**How it works:**
- `kFeatureDesktopIntegration` (bit 12) is requested through the existing
  HELLO feature marker and acknowledged like bits 2 to 11.
- `PICK_FILE` is accepted while the desktop is open. The regular Files
  selection opens over the desktop. While Files is open, AERA suspends
  desktop input: it sends a key-up for each held key, sends `RELEASE`,
  hides the desktop keyboard and gives touches, mouse and keys to Files.
  AERA swallows a press that began on the guest until it is released. A
  press that began on Files ends on Files. Every close path (select,
  cancel, new page, leaving the view, worker stop) restores input and the
  keyboard the worker asked for.
- `SET_DESKTOP_KEYBOARD` (worker kind 29) shows or hides the keyboard. The
  flags select the text, number (`kInputNumber`) or multiline
  (`kInputMultiline`) layout. Contacts above the keyboard still reach the
  guest. Done sends Return.
- `DESKTOP_KEYBOARD_INSET` (host kind 76) gives the number of framebuffer
  rows that the keyboard covers. AERA sends 0 after `DESKTOP_READY`. Then
  AERA sends the value each time the keyboard is shown or hidden, also
  when the user presses Done, uses the toolbar `KEYS` button (it now
  toggles) or opens Files.
- An on-screen key without an ASCII keysym is sent as an X11 keysym: the
  code point for Latin-1, else `0x01000000 | code point`.
- The session closes a worker that sends kind 29 without the bit. AERA
  never sends kind 76 to such a worker.

**Unchanged without the bit:** ACK flags, the refused `PICK_FILE` over the
desktop, the ASCII-only keyboard, the keyboard taking all input while it
is shown, and no geometry messages. `plugin_api_session_test` checks the
legacy ACK and the rejection of kind 29.

**Tests:** `plugin_api_session_test` (gating, numbers, unknown bit 15
dropped). `plugin_runtime_ui_test` (keysym table; Files over an open
desktop: suspension, inset 0 then restored, presses that span the modal,
multiline relayout).

**Status:** not built, not run on hardware. The host check runs the unit
tests above.

**Questions for maintainers:**
1. Are kinds 29 and 76 acceptable as the next free numbers?
2. Should Done in the single-line layout send Return, or only hide the
   keyboard as it does today?
