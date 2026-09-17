#[cfg(unix)]
fn restore_default_sigpipe() {
    // Rust 默认忽略 SIGPIPE，并把 EPIPE 转成 println! panic；传统 Unix CLI
    // 应在下游（如 `head`）提前关闭管道时安静退出。项目保持零依赖，直接调用 libc ABI。
    unsafe {
        extern "C" {
            fn signal(sig: i32, handler: usize) -> usize;
        }
        const SIGPIPE: i32 = 13;
        const SIG_DFL: usize = 0;
        let _ = signal(SIGPIPE, SIG_DFL);
    }
}

#[cfg(not(unix))]
fn restore_default_sigpipe() {}

fn main() {
    restore_default_sigpipe();
    std::process::exit(nopwd::cli::run());
}
