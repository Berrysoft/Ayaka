import 'ffi.dart';

class ManagedRuntime {
  final Runtime runtime;

  const ManagedRuntime({required this.runtime});

  Future<ManagedRuntime> newManagedRuntime() async {
    FlutterSettingsManager s =
        await FlutterSettingsManager.newFlutterSettingsManager(bridge: api);
    final runtime = await Runtime.newRuntime(bridge: api, s: s);
    return ManagedRuntime(runtime: runtime);
  }
}
