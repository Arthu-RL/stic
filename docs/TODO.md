# Stic – Planned Features & Known Gaps

This file tracks features that are **not yet implemented** or only partially wired.
See the [README](../README.md) for what works today.

---

# TODO TASKS:

Error when closing app, it seems it is leaking thread

```
Error: client exited without proper shutdown sequence

Stack backtrace:
   0: <anyhow::Error>::msg::<alloc::string::String>
   1: anyhow::__private::format_err
   2: rust_analyzer::run_server
   3: std::sys::backtrace::__rust_begin_short_backtrace::<<stdx::thread::Builder>::spawn<rust_analyzer::run_server, core::result::Result<(), anyhow::Error>>::{closure#0}, core::result::Result<(), anyhow::Error>>
   4: <<std::thread::Builder>::spawn_unchecked_<<stdx::thread::Builder>::spawn<rust_analyzer::run_server, core::result::Result<(), anyhow::Error>>::{closure#0}, core::result::Result<(), anyhow::Error>>::{closure#1} as core::ops::function::FnOnce<()>>::call_once::{shim:vtable#0}
   5: std::sys::thread::unix::Thread::new::thread_start
   6: start_thread
             at ./nptl/pthread_create.c:447:8
   7: clone3
             at ./misc/../sysdeps/unix/sysv/linux/x86_64/clone3.S:78:0
```

Have a way to save termianl session in app scope, so we can code in app running it at the same time in terminal mode