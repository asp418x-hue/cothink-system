// This is a basic Flutter widget test.
//
// To perform an interaction with a widget in your test, use the WidgetTester
// utility in the flutter_test package. For example, you can send tap and scroll
// gestures. You can also use WidgetTester to find child widgets in the widget
// tree, read text, and verify that the values of widget properties are correct.

import 'package:flutter_test/flutter_test.dart';
import 'package:cothink_core/main.dart';
import 'package:cothink_core/services/dashboard_controller.dart';

void main() {
  testWidgets('CothinkApp smoke test', (WidgetTester tester) async {
    final controller = DashboardController(autoStart: false);
    await tester.pumpWidget(CothinkApp(dashboardController: controller));
    expect(find.byType(CothinkApp), findsOneWidget);
    controller.dispose();
  });
}
