# aera_window

flutter-aera reports a small fixed padding (`src/view.rs`, `SIDE_DP` and
`BOTTOM_DP`) on the `aera/window` channel, because Flutter's stock embedder
API has no `viewPadding` field. `AeraWindowPadding` applies it as
`MediaQuery` padding, so `SafeArea` and `Scaffold` keep controls off a
phone's rounded corners on AERA. Elsewhere nothing is sent and it changes nothing.

Apps write nothing: flutter_p0g's AERA build compiles a generated
entrypoint that installs `AeraWindowBinding` before the app's own `main`,
and that binding puts `runApp`'s root inside `AeraWindowPadding`:

```dart
import 'package:aera_window/aera_window.dart' as aera;
import 'package:<app>/main.dart' as app;

void main(List<String> args) {
  aera.AeraWindowBinding.install();
  app.main(); // or app.main(args), matching the app's signature
}
```

`View.of(context).viewPadding` still reads zero (the stock engine has no
padding field); everything that reads `MediaQuery`, `SafeArea` and
`Scaffold` included, gets the padding. The first frame is laid out before
the padding arrives, a moment later.
