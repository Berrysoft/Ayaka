# Script plugin
All plugins are script plugins.
[Scripts](../config/script.md) could call methods in the script plugins.

## Calling methods
A plugin method is referenced with `<module>.<fn>(...)` grammar.
If you would like to call the `rnd` function in `random` module,
``` yaml
- \exec{random.rnd()}
```
Pass the parameters in the brace `()`.

As the scripts are calculated at runtime, if there's no plugin called `random`,
or no method called `rnd` inside `random`, it will give a warning, and continue with `RawValue::Unit`.

## Author a script plugin
Here we're going to author a script plugin `meet` to return a string "Hello".
``` rust,ignore
use ayaka_bindings::*;

#[export]
fn plugin_type() -> PluginType {
    PluginType::default()
}

#[export]
fn hello(_args: Vec<RawValue>) -> RawValue {
    RawValue::Str("Hello".to_string())
}
```
And call the function:
``` yaml
- \exec{meet.hello()} from plugin!
```
If it builds successfully, and you set the right path to the plugins, it will output:
``` ignore
Hello from plugin!
```

## Existing plugins
There are some existing script (only) plugins:

| Plugin   | Description              |
| -------- | ------------------------ |
| `format` | Format strings.          |
| `ayalog` | Log to runtime.          |
| `random` | Generate random numbers. |
