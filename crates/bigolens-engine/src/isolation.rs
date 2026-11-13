//! Linux platform isolation: unprivileged namespaces (user, mount, network)
//! and seccomp-bpf syscall allow-lists.
//!
//! Conforms to ADR 0014 and BL-405.

use std::io;
use std::sync::OnceLock;

#[cfg(target_os = "linux")]
const PR_SET_NO_NEW_PRIVS: libc::c_int = 38;
#[cfg(target_os = "linux")]
const PR_SET_SECCOMP: libc::c_int = 22;
#[cfg(target_os = "linux")]
const SECCOMP_MODE_FILTER: libc::c_long = 2;
#[cfg(target_os = "linux")]
const SECCOMP_RET_ALLOW: u32 = 0x7fff_0000;
#[cfg(target_os = "linux")]
const SECCOMP_RET_ERRNO: u32 = 0x0005_0000;
#[cfg(target_os = "linux")]
const EPERM: u32 = 1;

#[cfg(target_os = "linux")]
const BPF_LD: u16 = 0x00;
#[cfg(target_os = "linux")]
const BPF_W: u16 = 0x00;
#[cfg(target_os = "linux")]
const BPF_ABS: u16 = 0x20;
#[cfg(target_os = "linux")]
const BPF_JMP: u16 = 0x05;
#[cfg(target_os = "linux")]
const BPF_JEQ: u16 = 0x15;
#[cfg(target_os = "linux")]
const BPF_K: u16 = 0x00;
#[cfg(target_os = "linux")]
const BPF_RET: u16 = 0x06;

#[repr(C)]
#[derive(Debug, Clone, Copy)]
pub struct SockFilter {
    pub code: u16,
    pub jt: u8,
    pub jf: u8,
    pub k: u32,
}

#[repr(C)]
#[derive(Debug, Clone, Copy)]
pub struct SockFprog {
    pub len: u16,
    pub filter: *const SockFilter,
}

#[cfg(target_os = "linux")]
unsafe fn format_u32_map(id: u32, buf: &mut [u8; 64]) -> usize {
    let mut temp = [0u8; 16];
    let mut val = id;
    let mut idx = 0;
    if val == 0 {
        temp[0] = b'0';
        idx = 1;
    } else {
        while val > 0 {
            temp[idx] = b'0' + (val % 10) as u8;
            val /= 10;
            idx += 1;
        }
    }
    buf[0] = b'0';
    buf[1] = b' ';
    let mut out_idx = 2;
    for i in (0..idx).rev() {
        buf[out_idx] = temp[i];
        out_idx += 1;
    }
    buf[out_idx] = b' ';
    out_idx += 1;
    buf[out_idx] = b'1';
    out_idx += 1;
    out_idx
}

#[cfg(target_os = "linux")]
unsafe fn write_proc_map(path: &[u8], id: u32) -> Result<(), io::Error> {
    let fd = libc::open(path.as_ptr() as _, libc::O_WRONLY);
    if fd < 0 {
        return Err(io::Error::last_os_error());
    }
    let mut buf = [0u8; 64];
    let len = format_u32_map(id, &mut buf);
    let n = libc::write(fd, buf.as_ptr() as _, len);
    libc::close(fd);
    if n != len as isize {
        return Err(io::Error::last_os_error());
    }
    Ok(())
}

static ISOLATION_AVAILABLE: OnceLock<bool> = OnceLock::new();

/// Check whether the current Linux environment supports unprivileged user namespaces and isolation.
#[cfg(target_os = "linux")]
pub fn is_linux_isolation_available() -> bool {
    *ISOLATION_AVAILABLE.get_or_init(|| {
        let pid = unsafe { libc::fork() };
        if pid < 0 {
            return false;
        }
        if pid == 0 {
            unsafe {
                let uid = libc::getuid();
                let gid = libc::getgid();

                // Stage 1: unshare user namespace
                if libc::unshare(libc::CLONE_NEWUSER) != 0 {
                    libc::_exit(1);
                }

                // Map UID
                if write_proc_map(b"/proc/self/uid_map\0", uid).is_err() {
                    libc::_exit(2);
                }

                // Deny setgroups
                let sg_fd = libc::open(c"/proc/self/setgroups".as_ptr(), libc::O_WRONLY);
                if sg_fd < 0 || libc::write(sg_fd, b"deny".as_ptr() as _, 4) != 4 {
                    if sg_fd >= 0 {
                        libc::close(sg_fd);
                    }
                    libc::_exit(3);
                }
                libc::close(sg_fd);

                // Map GID
                if write_proc_map(b"/proc/self/gid_map\0", gid).is_err() {
                    libc::_exit(4);
                }

                // Stage 2: unshare network and mount namespaces
                if libc::unshare(libc::CLONE_NEWNET | libc::CLONE_NEWNS) != 0 {
                    libc::_exit(5);
                }

                // Mount tmpfs over /home
                let fstype = c"tmpfs".as_ptr();
                let target = c"/home".as_ptr();
                if libc::mount(fstype, target, fstype, 0, std::ptr::null()) != 0 {
                    libc::_exit(6);
                }

                // Probe PR_SET_NO_NEW_PRIVS
                if libc::prctl(PR_SET_NO_NEW_PRIVS, 1, 0, 0, 0) != 0 {
                    libc::_exit(7);
                }

                libc::_exit(0);
            }
        }

        let mut status = 0;
        unsafe { libc::waitpid(pid, &mut status, 0) };
        libc::WIFEXITED(status) && libc::WEXITSTATUS(status) == 0
    })
}

#[cfg(not(target_os = "linux"))]
pub fn is_linux_isolation_available() -> bool {
    false
}

/// Apply unprivileged user, network, and mount namespaces to isolate the benchmark child.
///
/// Disconnects network completely, mounts tmpfs over `/home` to obscure all user files,
/// and maps the process to unprivileged UID 0 inside its isolated namespace.
///
/// # Safety
///
/// Must be called only in a single-threaded process context (e.g. child process after `fork`).
#[cfg(target_os = "linux")]
pub unsafe fn apply_linux_namespaces() -> io::Result<()> {
    let uid = libc::getuid();
    let gid = libc::getgid();

    // Stage 1: unshare user namespace
    if libc::unshare(libc::CLONE_NEWUSER) != 0 {
        return Err(io::Error::last_os_error());
    }

    // Map UID
    write_proc_map(b"/proc/self/uid_map\0", uid)?;

    // Deny setgroups
    let sg_fd = libc::open(c"/proc/self/setgroups".as_ptr(), libc::O_WRONLY);
    if sg_fd < 0 {
        return Err(io::Error::last_os_error());
    }
    let res = libc::write(sg_fd, b"deny".as_ptr() as _, 4);
    libc::close(sg_fd);
    if res != 4 {
        return Err(io::Error::last_os_error());
    }

    // Map GID
    write_proc_map(b"/proc/self/gid_map\0", gid)?;

    // Stage 2: unshare network and mount namespaces
    if libc::unshare(libc::CLONE_NEWNET | libc::CLONE_NEWNS) != 0 {
        return Err(io::Error::last_os_error());
    }

    // Mount an empty tmpfs over /home to conceal user directories
    let fstype = c"tmpfs".as_ptr();
    let target = c"/home".as_ptr();
    let mount_res = libc::mount(fstype, target, fstype, 0, std::ptr::null());
    if mount_res != 0 {
        return Err(io::Error::last_os_error());
    }

    Ok(())
}

/// Fallback for non-Linux OS.
///
/// # Safety
///
/// Unused on non-Linux OS.
#[cfg(not(target_os = "linux"))]
pub unsafe fn apply_linux_namespaces() -> io::Result<()> {
    Err(io::Error::new(
        io::ErrorKind::Unsupported,
        "linux namespaces are only supported on Linux",
    ))
}

/// Apply a seccomp BPF filter that explicitly blocks sensitive syscalls
/// (`execve`, `socket`, `connect`, `bind`, `listen`, `accept`, `ptrace`).
///
/// # Safety
///
/// Must be called in a process that is about to execute untrusted child code.
#[cfg(target_os = "linux")]
pub unsafe fn apply_seccomp_filter() -> io::Result<()> {
    // PR_SET_NO_NEW_PRIVS is mandatory before unprivileged seccomp
    if libc::prctl(PR_SET_NO_NEW_PRIVS, 1, 0, 0, 0) != 0 {
        return Err(io::Error::last_os_error());
    }

    // Determine blocked syscalls based on CPU architecture
    // Note: execveat is permitted for initial binary launch via fexecve/execveat
    let blocked_syscalls: &'static [u32] = if cfg!(target_arch = "x86_64") {
        &[
            59,  // execve
            41,  // socket
            42,  // connect
            49,  // bind
            50,  // listen
            43,  // accept
            101, // ptrace
        ]
    } else if cfg!(target_arch = "aarch64") {
        &[
            221, // execve
            198, // socket
            203, // connect
            200, // bind
            201, // listen
            202, // accept
            117, // ptrace
        ]
    } else {
        &[]
    };

    if blocked_syscalls.is_empty() {
        return Ok(());
    }

    let mut instructions = Vec::new();
    // 0: Load syscall number from seccomp_data: offsetof(seccomp_data, nr) == 0
    instructions.push(SockFilter {
        code: BPF_LD | BPF_W | BPF_ABS,
        jt: 0,
        jf: 0,
        k: 0,
    });

    let n = blocked_syscalls.len();
    for (i, &sc) in blocked_syscalls.iter().enumerate() {
        let remaining_checks = (n - 1 - i) as u8;
        instructions.push(SockFilter {
            code: BPF_JMP | BPF_JEQ | BPF_K,
            jt: remaining_checks + 1,
            jf: 0,
            k: sc,
        });
    }

    // Default: allow syscall
    instructions.push(SockFilter {
        code: BPF_RET | BPF_K,
        jt: 0,
        jf: 0,
        k: SECCOMP_RET_ALLOW,
    });

    // Deny: return EPERM
    instructions.push(SockFilter {
        code: BPF_RET | BPF_K,
        jt: 0,
        jf: 0,
        k: SECCOMP_RET_ERRNO | EPERM,
    });

    let fprog = SockFprog {
        len: instructions.len() as u16,
        filter: instructions.as_ptr(),
    };

    let ret = libc::prctl(PR_SET_SECCOMP, SECCOMP_MODE_FILTER, &fprog);
    if ret != 0 {
        return Err(io::Error::last_os_error());
    }

    Ok(())
}

/// Fallback for non-Linux OS.
///
/// # Safety
///
/// Unused on non-Linux OS.
#[cfg(not(target_os = "linux"))]
pub unsafe fn apply_seccomp_filter() -> io::Result<()> {
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    #[cfg(target_os = "linux")]
    fn test_linux_isolation_availability() {
        // Probe must complete and return boolean without crashing or hanging
        let _ = is_linux_isolation_available();
    }
}
