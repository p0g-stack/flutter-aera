// Host-only API 2 simulator fixture. No device discovery or native writes.
import 'package:file_selector/file_selector.dart';
import 'package:flutter/material.dart';

void main() => runApp(const MaterialApp(home: Probe()));

class Probe extends StatefulWidget {
  const Probe({super.key});
  @override
  State<Probe> createState() => _ProbeState();
}

class _ProbeState extends State<Probe> {
  int count = 0;
  String submitted = '';
  String selected = '';

  Future<void> pick() async {
    try {
      final file = await openFile();
      if (!mounted) return;
      setState(() => selected = file?.path ?? 'cancelled');
      print('api2: picked: $selected');
    } catch (error) {
      print('api2: picker error: $error');
    }
  }

  @override
  Widget build(BuildContext context) {
    final inset = MediaQuery.viewInsetsOf(context).bottom.round();
    print('api2: inset: $inset');
    return Scaffold(
      appBar: AppBar(title: const Text('API 2 desktop probe')),
      body: SingleChildScrollView(
        padding: const EdgeInsets.fromLTRB(24, 24, 24, 0),
        child: Column(crossAxisAlignment: CrossAxisAlignment.start, children: [
          TextField(
            key: const ValueKey('text'),
            decoration: const InputDecoration(labelText: 'Name'),
            onSubmitted: (value) {
              print('api2: submitted: $value');
              setState(() => submitted = value);
            },
          ),
          const SizedBox(height: 16),
          Text('Submitted: $submitted'),
          Text('Keyboard inset: $inset'),
          const SizedBox(height: 16),
          ElevatedButton(key: const ValueKey('pick'), onPressed: pick,
              child: const Text('Pick a file')),
          Text('Selected: $selected'),
          ElevatedButton(
            key: const ValueKey('details'),
            onPressed: () async {
              await Navigator.of(context).push<void>(MaterialPageRoute(
                builder: (_) => const Scaffold(body: Center(child: Text('Details'))),
              ));
              print('api2: back returned');
            },
            child: const Text('Details'),
          ),
          Text('Count: $count', style: const TextStyle(fontSize: 28)),
        ]),
      ),
      floatingActionButton: FloatingActionButton(
        key: const ValueKey('increment'),
        onPressed: () {
          setState(() => count++);
          print('api2: count: $count');
        },
        child: const Icon(Icons.add),
      ),
    );
  }
}
