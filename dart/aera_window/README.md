# aera_window

flutter-aera reports a small fixed padding (`src/view.rs`, `SIDE_DP` and
`BOTTOM_DP`) on the `aera/window` channel, because Flutter's stock embedder
API has no `viewPadding` field. `AeraWindowPadding` applies it as
`MediaQuery` padding, so `SafeArea` and `Scaffold` keep controls off a
phone's rounded corners on AERA. Wrap the app with it on the AERA place
only; elsewhere nothing is sent and it changes nothing.

```dart
AeraWindowPadding(child: app)
```
