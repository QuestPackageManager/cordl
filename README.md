# cordl

Cordl (pardon the odd name) is a tool that parses global metadata, runtime registration and code registration information from a Unity game generated using IL2CPP. 

It allows static analysis of various informations provided by IL2CPP such as class info, methods and others. Cordl offers the following generation outputs:
- C++ header only
- Rust crate
- JSON

This is a frontend for [brocolib](https://github.com/StackDoubleFlow/brocolib).

As it stands, brocolib only supports ARM instructions and is intended for Android use. This may be expanded further on.

# Usage

Requires Ubuntu 22.04 (lib6 requirement)

While this may generate headers for Android IL2CPP (or any future supported platforms), you are highly suggested to run this under WSL for convenience.

`libclang` is required for building the binary. 

# Running

Find the `global-metadata.dat` and `libil2cpp.so` in your IL2CPP binary and run:
```
cargo run --features il2cpp_v31 --metadata ./global-metadata.dat --libil2cpp ./libil2cpp.so cpp
```

Change `cpp` to the target generation of your choosing. Use the `json` target if you wish to use it for other means.