import 'package:flutter/services.dart';
import 'package:flutter/widgets.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:aera_window/aera_window.dart';

void main() {
  testWidgets('applies the embedder padding and follows updates', (tester) async {
    tester.view.devicePixelRatio = 3;
    addTearDown(tester.view.reset);
    const codec = JSONMessageCodec();
    var current = {'viewPadding': {'left': 32, 'top': 0, 'right': 32, 'bottom': 60}};
    tester.binding.defaultBinaryMessenger.setMockMessageHandler('aera/window', (_) async => codec.encodeMessage(current));
    late EdgeInsets padding;
    await tester.pumpWidget(AeraWindowPadding(child: Builder(builder: (context) {
      padding = MediaQuery.paddingOf(context);
      return const SizedBox();
    })));
    await tester.pump();
    expect(padding, const EdgeInsets.fromLTRB(32 / 3, 0, 32 / 3, 20));
    // A rotation: the embedder pushes the new padding.
    current = {'viewPadding': {'left': 69, 'top': 0, 'right': 69, 'bottom': 60}};
    await tester.binding.defaultBinaryMessenger.handlePlatformMessage('aera/window', codec.encodeMessage(current), (_) {});
    await tester.pump();
    expect(padding, const EdgeInsets.fromLTRB(23, 0, 23, 20));
  });

  testWidgets('the keyboard takes the bottom padding', (tester) async {
    tester.view.devicePixelRatio = 3;
    tester.view.viewInsets = const FakeViewPadding(bottom: 900);
    addTearDown(tester.view.reset);
    const codec = JSONMessageCodec();
    tester.binding.defaultBinaryMessenger.setMockMessageHandler('aera/window', (_) async => codec.encodeMessage({'viewPadding': {'left': 32, 'top': 0, 'right': 32, 'bottom': 60}}));
    late MediaQueryData data;
    await tester.pumpWidget(AeraWindowPadding(child: Builder(builder: (context) {
      data = MediaQuery.of(context);
      return const SizedBox();
    })));
    await tester.pump();
    expect(data.padding.bottom, 0);
    expect(data.viewPadding.bottom, 20);
  });
}
