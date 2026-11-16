//! Linux platform isolation: unprivileged namespaces (user, mount, network, pid),
//! capability dropping, and hardened seccomp-bpf syscall deny-list with argument filtering.
//!
//! Conforms to ADR 0014 and BL-405.

use std::io;
use std::path::Path;
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
const SECCOMP_RET_KILL_PROCESS: u32 = 0x8000_0000;
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
const BPF_JGE: u16 = 0x35;
#[cfg(target_os = "linux")]
const BPF_K: u16 = 0x00;
#[cfg(target_os = "linux")]
const BPF_RET: u16 = 0x06;

#[cfg(target_os = "linux")]
const AUDIT_ARCH_X86_64: u32 = 0xc000_003e;
#[cfg(target_os = "linux")]
const AUDIT_ARCH_AARCH64: u32 = 0xc000_00b7;
#[cfg(target_os = "linux")]
const X32_SYSCALL_BIT: u32 = 0x4000_0000;

#[repr(C)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
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

// Safety: SockFprog contains a raw pointer to an array of SockFilter that outlives the spawn.
// It is POD and safe to send/share across thread boundaries into the pre_exec closure.
unsafe impl Send for SockFprog {}
unsafe impl Sync for SockFprog {}

/// Owned BPF program built before fork, keeping pre_exec async-signal-safe.
#[derive(Debug, Clone)]
pub struct SeccompProgram {
    filters: Vec<SockFilter>,
}

impl SeccompProgram {
    pub fn as_fprog(&self) -> SockFprog {
        SockFprog {
            len: self.filters.len() as u16,
            filter: self.filters.as_ptr(),
        }
    }
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

#[cfg(target_os = "linux")]
const PR_CAPBSET_DROP: libc::c_int = 24;
#[cfg(target_os = "linux")]
const PR_SET_SECUREBITS: libc::c_int = 28;
#[cfg(target_os = "linux")]
const SECBIT_NOROOT: u32 = 1 << 0;
#[cfg(target_os = "linux")]
const SECBIT_NOROOT_LOCKED: u32 = 1 << 1;
#[cfg(target_os = "linux")]
const SECBIT_NO_SETUID_FIXUP: u32 = 1 << 2;
#[cfg(target_os = "linux")]
const SECBIT_NO_SETUID_FIXUP_LOCKED: u32 = 1 << 3;
#[cfg(target_os = "linux")]
const LINUX_CAPABILITY_VERSION_3: u32 = 0x2008_0522;

#[repr(C)]
struct CapUserHeader {
    version: u32,
    pid: i32,
}

#[repr(C)]
#[derive(Clone, Copy)]
struct CapUserData {
    effective: u32,
    permitted: u32,
    inheritable: u32,
}

/// Drop all Linux capabilities using raw, async-signal-safe syscalls.
///
/// Clears the capability bounding set via PR_CAPBSET_DROP, locks securebits
/// (SECBIT_NOROOT and SECBIT_NO_SETUID_FIXUP), enforces PR_SET_NO_NEW_PRIVS,
/// and clears effective, permitted, and inheritable capabilities via capset(2).
///
/// # Safety
///
/// Must be called in a single-threaded process context (e.g. after fork)
/// before executing the benchmark binary.
#[cfg(target_os = "linux")]
pub unsafe fn drop_all_capabilities() -> io::Result<()> {
    // 1. Drop every capability from the bounding set (caps 0..63)
    for cap in 0..64 {
        let _ = libc::prctl(PR_CAPBSET_DROP, cap, 0, 0, 0);
    }

    // 2. Lock securebits to prevent root or setuid regaining capabilities
    let secbits = SECBIT_NOROOT
        | SECBIT_NOROOT_LOCKED
        | SECBIT_NO_SETUID_FIXUP
        | SECBIT_NO_SETUID_FIXUP_LOCKED;
    let _ = libc::prctl(PR_SET_SECUREBITS, secbits, 0, 0, 0);

    // 3. Ensure no new privileges can be gained across execve
    if libc::prctl(PR_SET_NO_NEW_PRIVS, 1, 0, 0, 0) != 0 {
        return Err(io::Error::last_os_error());
    }

    // 4. Clear all effective, permitted, and inheritable capability bits
    let hdr = CapUserHeader {
        version: LINUX_CAPABILITY_VERSION_3,
        pid: 0,
    };
    let data = [
        CapUserData {
            effective: 0,
            permitted: 0,
            inheritable: 0,
        },
        CapUserData {
            effective: 0,
            permitted: 0,
            inheritable: 0,
        },
    ];
    if libc::syscall(libc::SYS_capset, &hdr, data.as_ptr()) != 0 {
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
pub unsafe fn drop_all_capabilities() -> io::Result<()> {
    Ok(())
}

static ISOLATION_AVAILABLE: OnceLock<bool> = OnceLock::new();

/// Check whether the current Linux environment supports unprivileged user namespaces and isolation.
#[cfg(target_os = "linux")]
pub fn is_linux_isolation_available() -> bool {
    // Fail closed on unsupported architectures
    if !cfg!(any(target_arch = "x86_64", target_arch = "aarch64")) {
        return false;
    }

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

                // Stage 2: unshare network, mount, and PID namespaces
                if libc::unshare(libc::CLONE_NEWNET | libc::CLONE_NEWNS | libc::CLONE_NEWPID) != 0 {
                    libc::_exit(5);
                }

                // Make mount namespace private
                let root = c"/".as_ptr();
                if libc::mount(
                    std::ptr::null(),
                    root,
                    std::ptr::null(),
                    libc::MS_REC | libc::MS_PRIVATE,
                    std::ptr::null(),
                ) != 0
                {
                    libc::_exit(6);
                }

                // Read-only root bind mount
                if libc::mount(
                    root,
                    root,
                    std::ptr::null(),
                    libc::MS_BIND | libc::MS_REC,
                    std::ptr::null(),
                ) != 0
                {
                    libc::_exit(7);
                }
                if libc::mount(
                    std::ptr::null(),
                    root,
                    std::ptr::null(),
                    libc::MS_BIND | libc::MS_REMOUNT | libc::MS_RDONLY | libc::MS_REC,
                    std::ptr::null(),
                ) != 0
                {
                    libc::_exit(8);
                }

                // Probe mounting /home tmpfs
                let tmpfs = c"tmpfs".as_ptr();
                let home = c"/home".as_ptr();
                let mut st: libc::stat = std::mem::zeroed();
                if libc::stat(home, &mut st) == 0 {
                    if libc::mount(
                        tmpfs,
                        home,
                        tmpfs,
                        libc::MS_NOSUID | libc::MS_NODEV | libc::MS_NOEXEC,
                        std::ptr::null(),
                    ) != 0
                    {
                        libc::_exit(9);
                    }
                } else if io::Error::last_os_error().raw_os_error() != Some(libc::ENOENT) {
                    libc::_exit(9);
                }

                // Probe mounting /root tmpfs
                let root_home = c"/root".as_ptr();
                if libc::stat(root_home, &mut st) == 0 {
                    if libc::mount(
                        tmpfs,
                        root_home,
                        tmpfs,
                        libc::MS_NOSUID | libc::MS_NODEV | libc::MS_NOEXEC,
                        std::ptr::null(),
                    ) != 0
                    {
                        libc::_exit(10);
                    }
                } else if io::Error::last_os_error().raw_os_error() != Some(libc::ENOENT) {
                    libc::_exit(10);
                }

                // Probe masking /etc/passwd
                let dev_null = c"/dev/null".as_ptr();
                let etc_passwd = c"/etc/passwd".as_ptr();
                if libc::stat(etc_passwd, &mut st) == 0 {
                    if libc::mount(
                        dev_null,
                        etc_passwd,
                        std::ptr::null(),
                        libc::MS_BIND,
                        std::ptr::null(),
                    ) != 0
                    {
                        libc::_exit(11);
                    }
                } else if io::Error::last_os_error().raw_os_error() != Some(libc::ENOENT) {
                    libc::_exit(11);
                }

                // Probe /tmp tmpfs, /tmp/scratch tmpfs, and remount /tmp rdonly
                let tmp = c"/tmp".as_ptr();
                if libc::stat(tmp, &mut st) == 0 {
                    if libc::mount(
                        tmpfs,
                        tmp,
                        tmpfs,
                        libc::MS_NOSUID | libc::MS_NODEV,
                        std::ptr::null(),
                    ) != 0
                    {
                        libc::_exit(12);
                    }
                    let scratch = c"/tmp/scratch".as_ptr();
                    if libc::mkdir(scratch, 0o700) != 0
                        && io::Error::last_os_error().raw_os_error() != Some(libc::EEXIST)
                    {
                        libc::_exit(13);
                    }
                    if libc::mount(
                        tmpfs,
                        scratch,
                        tmpfs,
                        libc::MS_NOSUID | libc::MS_NODEV,
                        std::ptr::null(),
                    ) != 0
                    {
                        libc::_exit(14);
                    }
                    if libc::mount(
                        std::ptr::null(),
                        tmp,
                        std::ptr::null(),
                        libc::MS_REMOUNT | libc::MS_RDONLY | libc::MS_NOSUID | libc::MS_NODEV,
                        std::ptr::null(),
                    ) != 0
                    {
                        libc::_exit(15);
                    }
                    if libc::chdir(scratch) != 0 {
                        libc::_exit(16);
                    }
                } else if io::Error::last_os_error().raw_os_error() != Some(libc::ENOENT) {
                    libc::_exit(12);
                }

                // Probe PID namespace child and fresh /proc mount
                let child = libc::fork();
                if child < 0 {
                    libc::_exit(17);
                }
                if child == 0 {
                    let proc_target = c"/proc".as_ptr();
                    let proc_type = c"proc".as_ptr();
                    let proc_flags =
                        libc::MS_NOSUID | libc::MS_NODEV | libc::MS_NOEXEC | libc::MS_RDONLY;
                    let opts = c"hidepid=2".as_ptr() as *const libc::c_void;
                    if libc::mount(proc_type, proc_target, proc_type, proc_flags, opts) != 0
                        && libc::mount(
                            proc_type,
                            proc_target,
                            proc_type,
                            proc_flags,
                            std::ptr::null(),
                        ) != 0
                    {
                        libc::_exit(18);
                    }

                    // Probe capability drop
                    if drop_all_capabilities().is_err() {
                        libc::_exit(19);
                    }

                    // Probe seccomp BPF filter
                    if let Ok(prog) = build_seccomp_filter(1000) {
                        let fprog = prog.as_fprog();
                        if libc::prctl(PR_SET_SECCOMP, SECCOMP_MODE_FILTER, &fprog) != 0 {
                            libc::_exit(20);
                        }
                    } else {
                        libc::_exit(21);
                    }

                    libc::_exit(0);
                }

                let mut cstatus = 0;
                if libc::waitpid(child, &mut cstatus, 0) < 0
                    || !libc::WIFEXITED(cstatus)
                    || libc::WEXITSTATUS(cstatus) != 0
                {
                    libc::_exit(22);
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

/// Apply unprivileged user, network, mount, and PID namespaces to isolate the benchmark child.
///
/// Disconnects networking completely, remounts root read-only, conceals `/home`, `/root`, and `/etc/passwd`,
/// isolates `/tmp` with a private tmpfs containing a writable scratch dir, and maps the process to UID 0 inside.
///
/// # Safety
///
/// Must be called only in a single-threaded process context (e.g. child process after `fork`).
#[cfg(target_os = "linux")]
pub unsafe fn apply_linux_namespaces(_scratch_dir: Option<&Path>) -> io::Result<()> {
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

    // Stage 2: unshare network, mount, and PID namespaces
    if libc::unshare(libc::CLONE_NEWNET | libc::CLONE_NEWNS | libc::CLONE_NEWPID) != 0 {
        return Err(io::Error::last_os_error());
    }

    // 1. Make mount namespace private so mount events do not propagate to host
    let root = c"/".as_ptr();
    if libc::mount(
        std::ptr::null(),
        root,
        std::ptr::null(),
        libc::MS_REC | libc::MS_PRIVATE,
        std::ptr::null(),
    ) != 0
    {
        return Err(io::Error::last_os_error());
    }

    // 2. Read-only root bind mount
    if libc::mount(
        root,
        root,
        std::ptr::null(),
        libc::MS_BIND | libc::MS_REC,
        std::ptr::null(),
    ) != 0
    {
        return Err(io::Error::last_os_error());
    }
    if libc::mount(
        std::ptr::null(),
        root,
        std::ptr::null(),
        libc::MS_BIND | libc::MS_REMOUNT | libc::MS_RDONLY | libc::MS_REC,
        std::ptr::null(),
    ) != 0
    {
        return Err(io::Error::last_os_error());
    }

    // 3. Mount an empty tmpfs over /home to conceal user directories (if present)
    let tmpfs = c"tmpfs".as_ptr();
    let home = c"/home".as_ptr();
    let mut st: libc::stat = std::mem::zeroed();
    if libc::stat(home, &mut st) == 0 {
        if libc::mount(
            tmpfs,
            home,
            tmpfs,
            libc::MS_NOSUID | libc::MS_NODEV | libc::MS_NOEXEC,
            std::ptr::null(),
        ) != 0
        {
            return Err(io::Error::last_os_error());
        }
    } else {
        let err = io::Error::last_os_error();
        if err.raw_os_error() != Some(libc::ENOENT) {
            return Err(err);
        }
    }

    // 4. Mount an empty tmpfs over /root to conceal root home (if present)
    let root_home = c"/root".as_ptr();
    if libc::stat(root_home, &mut st) == 0 {
        if libc::mount(
            tmpfs,
            root_home,
            tmpfs,
            libc::MS_NOSUID | libc::MS_NODEV | libc::MS_NOEXEC,
            std::ptr::null(),
        ) != 0
        {
            return Err(io::Error::last_os_error());
        }
    } else {
        let err = io::Error::last_os_error();
        if err.raw_os_error() != Some(libc::ENOENT) {
            return Err(err);
        }
    }

    // 5. Mask /etc/passwd by bind mounting /dev/null over it (if present)
    let dev_null = c"/dev/null".as_ptr();
    let etc_passwd = c"/etc/passwd".as_ptr();
    if libc::stat(etc_passwd, &mut st) == 0 {
        if libc::mount(
            dev_null,
            etc_passwd,
            std::ptr::null(),
            libc::MS_BIND,
            std::ptr::null(),
        ) != 0
        {
            return Err(io::Error::last_os_error());
        }
    } else {
        let err = io::Error::last_os_error();
        if err.raw_os_error() != Some(libc::ENOENT) {
            return Err(err);
        }
    }

    // 6. Private /tmp tmpfs, /tmp/scratch writable tmpfs, remount /tmp rdonly, chdir /tmp/scratch
    let tmp = c"/tmp".as_ptr();
    if libc::stat(tmp, &mut st) == 0 {
        if libc::mount(
            tmpfs,
            tmp,
            tmpfs,
            libc::MS_NOSUID | libc::MS_NODEV,
            std::ptr::null(),
        ) != 0
        {
            return Err(io::Error::last_os_error());
        }
        let scratch = c"/tmp/scratch".as_ptr();
        if libc::mkdir(scratch, 0o700) != 0 {
            let err = io::Error::last_os_error();
            if err.raw_os_error() != Some(libc::EEXIST) {
                return Err(err);
            }
        }
        if libc::mount(
            tmpfs,
            scratch,
            tmpfs,
            libc::MS_NOSUID | libc::MS_NODEV,
            std::ptr::null(),
        ) != 0
        {
            return Err(io::Error::last_os_error());
        }
        if libc::mount(
            std::ptr::null(),
            tmp,
            std::ptr::null(),
            libc::MS_REMOUNT | libc::MS_RDONLY | libc::MS_NOSUID | libc::MS_NODEV,
            std::ptr::null(),
        ) != 0
        {
            return Err(io::Error::last_os_error());
        }
        if libc::chdir(scratch) != 0 {
            return Err(io::Error::last_os_error());
        }
    } else {
        let err = io::Error::last_os_error();
        if err.raw_os_error() != Some(libc::ENOENT) {
            return Err(err);
        }
    }

    Ok(())
}

/// Fallback for non-Linux OS.
///
/// # Safety
///
/// Unused on non-Linux OS.
#[cfg(not(target_os = "linux"))]
pub unsafe fn apply_linux_namespaces(_scratch_dir: Option<&Path>) -> io::Result<()> {
    Err(io::Error::new(
        io::ErrorKind::Unsupported,
        "linux namespaces are only supported on Linux",
    ))
}

/// Build a seccomp BPF program for benchmark isolation.
///
/// Properties:
/// 1. Verifies AUDIT_ARCH for the native CPU architecture; kills on mismatch.
/// 2. On x86_64, rejects the x32 ABI (bit 0x40000000); kills on invocation.
/// 3. Denies execve, socket, connect, bind, listen, accept, accept4, ptrace with EPERM.
/// 4. Filters execveat: permits launch only via `allowed_fd` with `AT_EMPTY_PATH`.
///    All other execveat calls are denied with EPERM.
/// 5. On unsupported CPU architectures, fails closed with an error.
#[cfg(target_os = "linux")]
pub fn build_seccomp_filter(allowed_fd: i32) -> io::Result<SeccompProgram> {
    let native_arch = if cfg!(target_arch = "x86_64") {
        AUDIT_ARCH_X86_64
    } else if cfg!(target_arch = "aarch64") {
        AUDIT_ARCH_AARCH64
    } else {
        return Err(io::Error::new(
            io::ErrorKind::Unsupported,
            "seccomp architecture check unsupported on this CPU architecture; failing closed",
        ));
    };

    let (execve_nr, execveat_nr, blocked_syscalls): (u32, u32, &[u32]) =
        if cfg!(target_arch = "x86_64") {
            (
                59,  // execve
                322, // execveat
                &[
                    41,  // socket
                    42,  // connect
                    43,  // accept
                    49,  // bind
                    50,  // listen
                    101, // ptrace
                    155, // pivot_root
                    161, // chroot
                    165, // mount
                    166, // umount2
                    250, // keyctl
                    272, // unshare
                    288, // accept4
                    298, // perf_event_open
                    304, // open_by_handle_at
                    308, // setns
                    310, // process_vm_readv
                    311, // process_vm_writev
                    321, // bpf
                    323, // userfaultfd
                    425, // io_uring_setup
                    435, // clone3
                ],
            )
        } else if cfg!(target_arch = "aarch64") {
            (
                221, // execve
                281, // execveat
                &[
                    39,  // umount2
                    40,  // mount
                    41,  // pivot_root
                    51,  // chroot
                    97,  // unshare
                    117, // ptrace
                    198, // socket
                    200, // bind
                    201, // listen
                    202, // accept
                    203, // connect
                    219, // keyctl
                    241, // perf_event_open
                    242, // accept4
                    265, // open_by_handle_at
                    268, // setns
                    270, // process_vm_readv
                    271, // process_vm_writev
                    280, // bpf
                    282, // userfaultfd
                    425, // io_uring_setup
                    435, // clone3
                ],
            )
        } else {
            return Err(io::Error::new(
                io::ErrorKind::Unsupported,
                "unsupported CPU architecture for seccomp",
            ));
        };

    enum JumpTarget {
        Insn(usize),
        Deny,
        Kill,
        Allow,
    }

    struct RawInsn {
        code: u16,
        jt: JumpTarget,
        jf: JumpTarget,
        k: u32,
    }

    let mut raw: Vec<RawInsn> = Vec::new();

    // 1. Arch check: offsetof(seccomp_data, arch) == 4
    raw.push(RawInsn {
        code: BPF_LD | BPF_W | BPF_ABS,
        jt: JumpTarget::Insn(0),
        jf: JumpTarget::Insn(0),
        k: 4,
    });
    raw.push(RawInsn {
        code: BPF_JMP | BPF_JEQ | BPF_K,
        jt: JumpTarget::Insn(2),
        jf: JumpTarget::Kill,
        k: native_arch,
    });

    // 2. Load syscall number: offsetof(seccomp_data, nr) == 0
    raw.push(RawInsn {
        code: BPF_LD | BPF_W | BPF_ABS,
        jt: JumpTarget::Insn(0),
        jf: JumpTarget::Insn(0),
        k: 0,
    });

    // 3. On x86_64, check x32 ABI bit (nr >= 0x40000000)
    if cfg!(target_arch = "x86_64") {
        let next = raw.len() + 1;
        raw.push(RawInsn {
            code: BPF_JMP | BPF_JGE | BPF_K,
            jt: JumpTarget::Kill,
            jf: JumpTarget::Insn(next),
            k: X32_SYSCALL_BIT,
        });
    }

    // 4. Deny execve
    let next = raw.len() + 1;
    raw.push(RawInsn {
        code: BPF_JMP | BPF_JEQ | BPF_K,
        jt: JumpTarget::Deny,
        jf: JumpTarget::Insn(next),
        k: execve_nr,
    });

    // 5. Deny other blocked syscalls
    for &sc in blocked_syscalls {
        let next = raw.len() + 1;
        raw.push(RawInsn {
            code: BPF_JMP | BPF_JEQ | BPF_K,
            jt: JumpTarget::Deny,
            jf: JumpTarget::Insn(next),
            k: sc,
        });
    }

    // 6. Check execveat
    let arg_check = raw.len() + 1;
    raw.push(RawInsn {
        code: BPF_JMP | BPF_JEQ | BPF_K,
        jt: JumpTarget::Insn(arg_check),
        jf: JumpTarget::Allow,
        k: execveat_nr,
    });

    // args[0] low 32 bits (offset 16): allowed_fd
    let next = raw.len() + 1;
    raw.push(RawInsn {
        code: BPF_LD | BPF_W | BPF_ABS,
        jt: JumpTarget::Insn(0),
        jf: JumpTarget::Insn(0),
        k: 16,
    });
    raw.push(RawInsn {
        code: BPF_JMP | BPF_JEQ | BPF_K,
        jt: JumpTarget::Insn(next + 1),
        jf: JumpTarget::Deny,
        k: allowed_fd as u32,
    });

    // args[0] high 32 bits (offset 20): 0
    let next = raw.len() + 1;
    raw.push(RawInsn {
        code: BPF_LD | BPF_W | BPF_ABS,
        jt: JumpTarget::Insn(0),
        jf: JumpTarget::Insn(0),
        k: 20,
    });
    raw.push(RawInsn {
        code: BPF_JMP | BPF_JEQ | BPF_K,
        jt: JumpTarget::Insn(next + 1),
        jf: JumpTarget::Deny,
        k: 0,
    });

    // args[4] (flags) low 32 bits (offset 48): AT_EMPTY_PATH (0x1000)
    raw.push(RawInsn {
        code: BPF_LD | BPF_W | BPF_ABS,
        jt: JumpTarget::Insn(0),
        jf: JumpTarget::Insn(0),
        k: 48,
    });
    raw.push(RawInsn {
        code: BPF_JMP | BPF_JEQ | BPF_K,
        jt: JumpTarget::Allow,
        jf: JumpTarget::Deny,
        k: libc::AT_EMPTY_PATH as u32,
    });

    // Terminal instructions
    let deny_idx = raw.len();
    raw.push(RawInsn {
        code: BPF_RET | BPF_K,
        jt: JumpTarget::Insn(0),
        jf: JumpTarget::Insn(0),
        k: SECCOMP_RET_ERRNO | EPERM,
    });

    let kill_idx = raw.len();
    raw.push(RawInsn {
        code: BPF_RET | BPF_K,
        jt: JumpTarget::Insn(0),
        jf: JumpTarget::Insn(0),
        k: SECCOMP_RET_KILL_PROCESS,
    });

    let allow_idx = raw.len();
    raw.push(RawInsn {
        code: BPF_RET | BPF_K,
        jt: JumpTarget::Insn(0),
        jf: JumpTarget::Insn(0),
        k: SECCOMP_RET_ALLOW,
    });

    // Resolve jump offsets
    let mut filters = Vec::with_capacity(raw.len());
    for (idx, insn) in raw.iter().enumerate() {
        let resolve = |target: &JumpTarget| -> u8 {
            let dest = match target {
                JumpTarget::Insn(d) => *d,
                JumpTarget::Deny => deny_idx,
                JumpTarget::Kill => kill_idx,
                JumpTarget::Allow => allow_idx,
            };
            if dest <= idx {
                0
            } else {
                (dest - idx - 1) as u8
            }
        };

        filters.push(SockFilter {
            code: insn.code,
            jt: resolve(&insn.jt),
            jf: resolve(&insn.jf),
            k: insn.k,
        });
    }

    Ok(SeccompProgram { filters })
}

#[cfg(not(target_os = "linux"))]
pub fn build_seccomp_filter(_allowed_fd: i32) -> io::Result<SeccompProgram> {
    Ok(SeccompProgram {
        filters: Vec::new(),
    })
}

/// Apply a pre-built seccomp BPF filter to the current process.
///
/// Async-signal-safe: performs zero heap allocations or formatting.
///
/// # Safety
///
/// Must be called in a process that is about to execute untrusted child code.
#[cfg(target_os = "linux")]
pub unsafe fn apply_seccomp_filter(fprog: &SockFprog) -> io::Result<()> {
    if libc::prctl(PR_SET_NO_NEW_PRIVS, 1, 0, 0, 0) != 0 {
        return Err(io::Error::last_os_error());
    }

    let ret = libc::prctl(
        PR_SET_SECCOMP,
        SECCOMP_MODE_FILTER,
        fprog as *const SockFprog as *const _,
    );
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
pub unsafe fn apply_seccomp_filter(_fprog: &SockFprog) -> io::Result<()> {
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

    #[test]
    #[cfg(target_os = "linux")]
    fn test_seccomp_filter_building() {
        let prog = build_seccomp_filter(1000).expect("failed to build filter");
        let fprog = prog.as_fprog();
        assert!(fprog.len > 0);
        assert!(!fprog.filter.is_null());
    }
}
