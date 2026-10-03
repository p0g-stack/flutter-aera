import 'dart:math' as math;

import 'package:flutter/services.dart';
import 'package:flutter/widgets.dart';

/// Applies the padding flutter-aera reports on `aera/window`, so `SafeArea`
/// and `MediaQuery.padding` keep controls off AERA's edge strips and a
/// phone's rounded corners. Flutter's embedder API cannot set `viewPadding`,
/// so the embedder sends it on this channel instead; wrap the app with this
/// on the AERA place only (elsewhere nothing is ever sent).
class AeraWindowPadding extends StatefulWidget {
  const AeraWindowPadding({super.key, required this.child});

  final Widget child;

  static const channel = BasicMessageChannel<Object?>('aera/window', JSONMessageCodec());

  @override
  State<AeraWindowPadding> createState() => _AeraWindowPaddingState();
}

class _AeraWindowPaddingState extends State<AeraWindowPadding> {
  /// In physical pixels, as the embedder sends it.
  EdgeInsets _physical = EdgeInsets.zero;

  @override
  void initState() {
    super.initState();
    AeraWindowPadding.channel.setMessageHandler((message) async {
      _apply(message);
      return null;
    });
    // The embedder answers any message with the current padding.
    AeraWindowPadding.channel.send(null).then(_apply, onError: (Object _) {});
  }

  void _apply(Object? message) {
    final padding = message is Map ? message['viewPadding'] : null;
    if (padding is! Map || !mounted) return;
    double side(String key) => (padding[key] as num?)?.toDouble() ?? 0;
    final physical = EdgeInsets.fromLTRB(side('left'), side('top'), side('right'), side('bottom'));
    if (physical != _physical) setState(() => _physical = physical);
  }

  @override
  void dispose() {
    AeraWindowPadding.channel.setMessageHandler(null);
    super.dispose();
  }

  @override
  Widget build(BuildContext context) {
    final data = MediaQuery.of(context);
    final reported = _physical / data.devicePixelRatio;
    final viewPadding = EdgeInsets.fromLTRB(
      math.max(data.viewPadding.left, reported.left),
      math.max(data.viewPadding.top, reported.top),
      math.max(data.viewPadding.right, reported.right),
      math.max(data.viewPadding.bottom, reported.bottom),
    );
    // As FlutterView.padding: the view padding not already covered by the
    // keyboard.
    final insets = data.viewInsets;
    final padding = EdgeInsets.fromLTRB(
      math.max(0, viewPadding.left - insets.left),
      math.max(0, viewPadding.top - insets.top),
      math.max(0, viewPadding.right - insets.right),
      math.max(0, viewPadding.bottom - insets.bottom),
    );
    return MediaQuery(
      data: data.copyWith(viewPadding: viewPadding, padding: padding),
      child: widget.child,
    );
  }
}

/// The widgets binding for an app on AERA: [runApp]'s root goes inside
/// [AeraWindowPadding], so the app needs no code of its own. flutter_p0g's
/// AERA build calls [install] in a generated entrypoint before the app's own
/// `main`; [runApp] then finds this binding already in place.
class AeraWindowBinding extends WidgetsFlutterBinding {
  static bool _installed = false;

  /// Creates this binding. Call it first, before any other binding exists;
  /// later calls do nothing.
  static WidgetsBinding install() {
    if (!_installed) {
      _installed = true;
      AeraWindowBinding();
    }
    return WidgetsBinding.instance;
  }

  @override
  Widget wrapWithDefaultView(Widget rootWidget) =>
      super.wrapWithDefaultView(AeraWindowPadding(child: rootWidget));
}
