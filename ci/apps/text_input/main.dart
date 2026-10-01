// ci/text-input-check.sh: a field typed into through AERA's keyboard. The
// prints are what the check reads from the log.
import 'package:flutter/material.dart';

void main() => runApp(const MaterialApp(home: TextInputPage()));

class TextInputPage extends StatefulWidget {
  const TextInputPage({super.key});
  @override
  State<TextInputPage> createState() => _TextInputPageState();
}

class _TextInputPageState extends State<TextInputPage> {
  String submitted = '';
  @override
  Widget build(BuildContext context) {
    final inset = MediaQuery.viewInsetsOf(context).bottom;
    print('inset: ${inset.round()}');
    return Scaffold(
      appBar: AppBar(title: const Text('Text input')),
      body: Padding(
        padding: const EdgeInsets.all(16),
        child: Column(children: [
          TextField(
            decoration: const InputDecoration(labelText: 'Name'),
            onSubmitted: (v) {
              print('submitted: $v');
              setState(() => submitted = v);
            },
          ),
          const SizedBox(height: 24),
          Text('Submitted: $submitted', style: const TextStyle(fontSize: 24)),
          Text('Keyboard inset: ${inset.round()}', style: const TextStyle(fontSize: 24)),
        ]),
      ),
    );
  }
}
