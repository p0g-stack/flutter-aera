# Draft PR body: native field editor over the desktop (bit 13)

**Patch:** `0002-plugin_api-let-a-worker-open-a-native-field-editor-o.patch`,
on `aera-16.0` at 1cabc509 plus 0001.

**Title:** plugin_api: let a worker open a native field editor over its desktop

**What:** Today the native editor for a Host API 2 text input opens only
when the user taps the field's card on the setup page. When the desktop
is open, it covers that page. So a desktop app that focuses a field
cannot get AERA's editor. This patch adds an optional feature bit for
that. It reuses the existing fields and `TEXT_INPUT_RESULT`.

**How it works:**
- `kFeatureDesktopTextInput` (bit 13) is requested through the HELLO
  feature marker.
- `EDIT_TEXT_INPUT` (worker kind 30): `request_id` is an existing field
  ID. All other fields are zero or empty. AERA opens the same editor that
  a tap on the card opens, also over an open desktop. Over the desktop,
  desktop input is suspended as for Files in 0001 until the editor closes
  by any path: Done, Cancel, Back, new page, leaving the view, worker stop,
  or destruction of the editor. If the field cannot be edited now, AERA
  sends `TEXT_INPUT_RESULT` with `kInputError`. A request for the field
  that is already being edited does nothing.
- `DISMISS_TEXT_INPUT` (worker kind 31) cancels the editor of that field.
  A dismiss for a field that is not being edited is ignored. Each opened
  editor thus ends with exactly one result.
- The 255-code-point limit stays and no drafts are sent. This is the
  native modal path. Incremental typing into a desktop app uses 0001's
  keyboard and `PLAY_KEY`.

**Unchanged without the bit:** ACK flags, and the editor opens only from
its card and never over the desktop. The session closes a worker that
sends kinds 30 or 31 without the bit.

**Tests:** `plugin_api_session_test` (gating, numbers).
`plugin_runtime_ui_test` (editor over the desktop: suspension, idempotent
edit, dismiss, a second dismiss with no reply, submit with non-ASCII
text, error for an unknown field, restore when the editor is destroyed).

**Status:** not built, not run on hardware.

**Question for maintainers:** should a dismiss for a closed editor get an
explicit reply instead of being ignored?
