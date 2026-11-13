//========================================================================
//
// Confine.cc
//
// DocSlurp: see Confine.h. Moved from pdfregions.cc.
//
// This file is licensed under the GPLv2 or later
//
// Copyright (C) 2026 DocSlurp
//
//========================================================================

#include "config.h"
#include <poppler-config.h>
#include "Confine.h"

#include <cerrno>
#include <cstdint>
#include <cstring>
#include <fcntl.h>
#include <vector>
#ifndef _WIN32
#    include <unistd.h>
#endif
#ifdef __linux__
#    include <cstddef>
#    include <linux/audit.h>
#    include <linux/filter.h>
#    include <linux/landlock.h>
#    include <linux/seccomp.h>
#    include <sched.h>
#    include <sys/ioctl.h>
#    include <sys/prctl.h>
#    include <sys/syscall.h>
#endif
#if WITH_FONTCONFIGURATION_FONTCONFIG
#    include <fontconfig/fontconfig.h>
#endif

#ifdef __linux__
// calls newer than some C library headers: the same numbers on x86_64 and aarch64
#    if defined(__x86_64__) || defined(__aarch64__)
#        ifndef SYS_fchmodat2
#            define SYS_fchmodat2 452
#        endif
#        ifndef SYS_setxattrat
#            define SYS_setxattrat 463
#        endif
#        ifndef SYS_removexattrat
#            define SYS_removexattrat 466
#        endif
#        ifndef SYS_file_setattr
#            define SYS_file_setattr 469
#        endif
#    endif

static int applyLandlock()
{
#    if defined(SYS_landlock_create_ruleset) && defined(SYS_landlock_restrict_self)
    const long abi = syscall(SYS_landlock_create_ruleset, nullptr, 0, LANDLOCK_CREATE_RULESET_VERSION);
    if (abi < 1) {
        return 0;
    }
    struct landlock_ruleset_attr attr;
    memset(&attr, 0, sizeof(attr));
    attr.handled_access_fs = LANDLOCK_ACCESS_FS_EXECUTE | LANDLOCK_ACCESS_FS_WRITE_FILE | LANDLOCK_ACCESS_FS_REMOVE_DIR | LANDLOCK_ACCESS_FS_REMOVE_FILE | LANDLOCK_ACCESS_FS_MAKE_CHAR | LANDLOCK_ACCESS_FS_MAKE_DIR
            | LANDLOCK_ACCESS_FS_MAKE_REG | LANDLOCK_ACCESS_FS_MAKE_SOCK | LANDLOCK_ACCESS_FS_MAKE_FIFO | LANDLOCK_ACCESS_FS_MAKE_BLOCK | LANDLOCK_ACCESS_FS_MAKE_SYM;
    if (abi >= 2) {
        attr.handled_access_fs |= LANDLOCK_ACCESS_FS_REFER;
    }
    if (abi >= 3) {
        attr.handled_access_fs |= LANDLOCK_ACCESS_FS_TRUNCATE;
    }
#        ifdef LANDLOCK_ACCESS_FS_IOCTL_DEV
    if (abi >= 5) {
        attr.handled_access_fs |= LANDLOCK_ACCESS_FS_IOCTL_DEV;
    }
#        endif
#        ifdef LANDLOCK_ACCESS_NET_BIND_TCP
    if (abi >= 4) {
        attr.handled_access_net = LANDLOCK_ACCESS_NET_BIND_TCP | LANDLOCK_ACCESS_NET_CONNECT_TCP;
    }
#        endif
#        ifdef LANDLOCK_SCOPE_SIGNAL
    if (abi >= 6) {
        attr.scoped = LANDLOCK_SCOPE_ABSTRACT_UNIX_SOCKET | LANDLOCK_SCOPE_SIGNAL;
    }
#        endif
    const int fd = static_cast<int>(syscall(SYS_landlock_create_ruleset, &attr, sizeof(attr), 0));
    if (fd < 0) {
        return 0;
    }
    const long rc = syscall(SYS_landlock_restrict_self, fd, 0);
    close(fd);
    return rc == 0 ? static_cast<int>(abi) : 0;
#    else
    return 0;
#    endif
}

static bool applySeccomp()
{
#    if defined(__x86_64__)
    constexpr uint32_t arch = AUDIT_ARCH_X86_64;
#    elif defined(__aarch64__)
    constexpr uint32_t arch = AUDIT_ARCH_AARCH64;
#    else
    return false;
#    endif
#    if defined(__x86_64__) || defined(__aarch64__)
    constexpr uint32_t eperm = SECCOMP_RET_ERRNO | (EPERM & SECCOMP_RET_DATA);
    constexpr uint32_t enosys = SECCOMP_RET_ERRNO | (ENOSYS & SECCOMP_RET_DATA);
    constexpr uint32_t writeFlags = O_WRONLY | O_RDWR | O_CREAT | O_TRUNC | O_APPEND;
    // Denied with EPERM. Beyond running programs, the network and other processes' memory:
    // every call that changes a file through a path or a descriptor without opening it for
    // writing (modes, owners, times, extended attributes, sizes, links and names; Landlock
    // covers most of these, truncate only from ABI 3, attributes and times never), signals to
    // other processes (Landlock scopes them only from ABI 6), and System V and POSIX IPC
    // objects and memfds (shared with, or a way around the filesystem rules).
    const uint32_t denied[] = {
        SYS_execve,
        SYS_execveat,
        SYS_socket,
        SYS_socketpair,
        SYS_connect,
        SYS_ptrace,
        SYS_process_vm_readv,
        SYS_process_vm_writev,
#        ifdef SYS_creat
        SYS_creat,
#        endif
#        ifdef SYS_chmod
        SYS_chmod,
#        endif
        SYS_fchmod,
        SYS_fchmodat,
#        ifdef SYS_fchmodat2
        SYS_fchmodat2,
#        endif
#        ifdef SYS_chown
        SYS_chown,
#        endif
#        ifdef SYS_lchown
        SYS_lchown,
#        endif
        SYS_fchown,
        SYS_fchownat,
#        ifdef SYS_utime
        SYS_utime,
#        endif
#        ifdef SYS_utimes
        SYS_utimes,
#        endif
#        ifdef SYS_futimesat
        SYS_futimesat,
#        endif
        SYS_utimensat,
        SYS_truncate,
        SYS_ftruncate,
        SYS_fallocate,
        SYS_setxattr,
        SYS_lsetxattr,
        SYS_fsetxattr,
        SYS_removexattr,
        SYS_lremovexattr,
        SYS_fremovexattr,
#        ifdef SYS_setxattrat
        SYS_setxattrat,
#        endif
#        ifdef SYS_removexattrat
        SYS_removexattrat,
#        endif
#        ifdef SYS_file_setattr
        SYS_file_setattr,
#        endif
#        ifdef SYS_unlink
        SYS_unlink,
#        endif
#        ifdef SYS_rename
        SYS_rename,
#        endif
#        ifdef SYS_mkdir
        SYS_mkdir,
#        endif
#        ifdef SYS_rmdir
        SYS_rmdir,
#        endif
#        ifdef SYS_link
        SYS_link,
#        endif
#        ifdef SYS_symlink
        SYS_symlink,
#        endif
#        ifdef SYS_mknod
        SYS_mknod,
#        endif
#        ifdef SYS_renameat
        SYS_renameat,
#        endif
        SYS_unlinkat,
        SYS_renameat2,
        SYS_mkdirat,
        SYS_linkat,
        SYS_symlinkat,
        SYS_mknodat,
        SYS_tkill,
        SYS_rt_sigqueueinfo,
        SYS_rt_tgsigqueueinfo,
#        ifdef SYS_pidfd_open
        SYS_pidfd_open,
#        endif
#        ifdef SYS_pidfd_send_signal
        SYS_pidfd_send_signal,
#        endif
#        ifdef SYS_pidfd_getfd
        SYS_pidfd_getfd,
#        endif
        SYS_shmget,
        SYS_shmat,
        SYS_shmctl,
        SYS_msgget,
        SYS_msgsnd,
        SYS_msgrcv,
        SYS_msgctl,
        SYS_semget,
        SYS_semop,
        SYS_semtimedop,
        SYS_semctl,
        SYS_memfd_create,
        SYS_mq_open,
        SYS_mq_unlink,
        SYS_mq_timedsend,
        SYS_mq_timedreceive,
        SYS_mq_notify,
        SYS_mq_getsetattr,
        // sessions, process groups, namespaces, the kernel keyrings and advisory locks (shared
        // with other processes) are never needed
        SYS_setsid,
        SYS_setpgid,
        SYS_unshare,
        SYS_setns,
        SYS_keyctl,
        SYS_add_key,
        SYS_request_key,
        SYS_flock,
#        ifdef SYS_fork
        SYS_fork,
#        endif
#        ifdef SYS_vfork
        SYS_vfork,
#        endif
    };
    std::vector<sock_filter> f;
    auto stmt = [&f](uint16_t code, uint32_t k) { f.push_back(BPF_STMT(code, k)); };
    auto jump = [&f](uint16_t code, uint32_t k, uint8_t jt, uint8_t jf) { f.push_back(BPF_JUMP(code, k, jt, jf)); };
    stmt(BPF_LD | BPF_W | BPF_ABS, offsetof(struct seccomp_data, arch));
    jump(BPF_JMP | BPF_JEQ | BPF_K, arch, 1, 0);
    stmt(BPF_RET | BPF_K, SECCOMP_RET_KILL_PROCESS);
    stmt(BPF_LD | BPF_W | BPF_ABS, offsetof(struct seccomp_data, nr));
#        ifdef __x86_64__
    // x32 syscalls on x86_64
    jump(BPF_JMP | BPF_JGE | BPF_K, 0x40000000u, 0, 1);
    stmt(BPF_RET | BPF_K, eperm);
#        endif
    for (uint32_t nr : denied) {
        jump(BPF_JMP | BPF_JEQ | BPF_K, nr, 0, 1);
        stmt(BPF_RET | BPF_K, eperm);
    }
#        ifdef SYS_openat2
    jump(BPF_JMP | BPF_JEQ | BPF_K, SYS_openat2, 0, 1);
    stmt(BPF_RET | BPF_K, enosys);
#        endif
    // io_uring performs socket, connect and open operations without their syscalls (and so
    // without the checks above): unavailable, as on a kernel built without it
#        ifdef SYS_io_uring_setup
    for (uint32_t nr : { uint32_t(SYS_io_uring_setup), uint32_t(SYS_io_uring_enter), uint32_t(SYS_io_uring_register) }) {
        jump(BPF_JMP | BPF_JEQ | BPF_K, nr, 0, 1);
        stmt(BPF_RET | BPF_K, enosys);
    }
#        endif
    // kill(pid, sig) and tgkill(tgid, tid, sig): only this process (a pid of 0 or -1 names a
    // group or every process); raise() and abort() use tgkill on this process
    const uint32_t self = static_cast<uint32_t>(getpid());
    for (uint32_t nr : { uint32_t(SYS_kill), uint32_t(SYS_tgkill) }) {
        jump(BPF_JMP | BPF_JEQ | BPF_K, nr, 0, 4);
        stmt(BPF_LD | BPF_W | BPF_ABS, offsetof(struct seccomp_data, args)); // low 32 bits of the pid
        jump(BPF_JMP | BPF_JEQ | BPF_K, self, 0, 1);
        stmt(BPF_RET | BPF_K, SECCOMP_RET_ALLOW);
        stmt(BPF_RET | BPF_K, eperm);
    }
    // calls that act on another process named by a pid argument (resource limits, priority,
    // scheduling, memory placement): only this process, as a pid of 0 or its own pid (an
    // RLIMIT_CPU set on a sibling of the same user kills it)
    auto selfOnly = [&](uint32_t nr, int pidArg) {
        jump(BPF_JMP | BPF_JEQ | BPF_K, nr, 0, 5);
        stmt(BPF_LD | BPF_W | BPF_ABS, offsetof(struct seccomp_data, args) + 8 * pidArg); // low 32 bits
        jump(BPF_JMP | BPF_JEQ | BPF_K, 0, 2, 0);
        jump(BPF_JMP | BPF_JEQ | BPF_K, self, 1, 0);
        stmt(BPF_RET | BPF_K, eperm);
        stmt(BPF_RET | BPF_K, SECCOMP_RET_ALLOW);
    };
    for (uint32_t nr : { uint32_t(SYS_prlimit64), uint32_t(SYS_sched_setaffinity), uint32_t(SYS_sched_setscheduler), uint32_t(SYS_sched_setparam), uint32_t(SYS_sched_setattr), uint32_t(SYS_migrate_pages), uint32_t(SYS_move_pages) }) {
        selfOnly(nr, 0);
    }
    // setpriority(which, who, prio) and ioprio_set(which, who, ioprio): a process (not a group
    // or a user), and only this one
    auto selfProcessOnly = [&](uint32_t nr, uint32_t whichProcess) {
        jump(BPF_JMP | BPF_JEQ | BPF_K, nr, 0, 7);
        stmt(BPF_LD | BPF_W | BPF_ABS, offsetof(struct seccomp_data, args));
        jump(BPF_JMP | BPF_JEQ | BPF_K, whichProcess, 0, 3);
        stmt(BPF_LD | BPF_W | BPF_ABS, offsetof(struct seccomp_data, args) + 8);
        jump(BPF_JMP | BPF_JEQ | BPF_K, 0, 2, 0);
        jump(BPF_JMP | BPF_JEQ | BPF_K, self, 1, 0);
        stmt(BPF_RET | BPF_K, eperm);
        stmt(BPF_RET | BPF_K, SECCOMP_RET_ALLOW);
    };
    selfProcessOnly(SYS_setpriority, 0); // PRIO_PROCESS
    selfProcessOnly(SYS_ioprio_set, 1); // IOPRIO_WHO_PROCESS
    // clone(flags, ...): threads only (CLONE_THREAD); a new process is never needed. clone3,
    // whose flags a filter cannot read, is unavailable (ENOSYS): the C library falls back to
    // clone for a thread
    jump(BPF_JMP | BPF_JEQ | BPF_K, SYS_clone, 0, 4);
    stmt(BPF_LD | BPF_W | BPF_ABS, offsetof(struct seccomp_data, args)); // flags, low 32 bits
    jump(BPF_JMP | BPF_JSET | BPF_K, CLONE_THREAD, 1, 0);
    stmt(BPF_RET | BPF_K, eperm);
    stmt(BPF_RET | BPF_K, SECCOMP_RET_ALLOW);
#        ifdef SYS_clone3
    jump(BPF_JMP | BPF_JEQ | BPF_K, SYS_clone3, 0, 1);
    stmt(BPF_RET | BPF_K, enosys);
#        endif
    // ioctl(fd, cmd, ...): a short allow-list (terminal and descriptor queries, non-blocking and
    // close-on-exec flags); anything else (file attribute flags, say, even on a read-only
    // descriptor) is denied
    {
        const uint32_t cmds[] = { TCGETS, TIOCGWINSZ, FIONREAD, FIONBIO, FIOCLEX, FIONCLEX };
        const uint8_t n = sizeof(cmds) / sizeof(cmds[0]);
        jump(BPF_JMP | BPF_JEQ | BPF_K, SYS_ioctl, 0, n + 3);
        stmt(BPF_LD | BPF_W | BPF_ABS, offsetof(struct seccomp_data, args) + 8); // cmd
        for (uint8_t i = 0; i < n; ++i) {
            jump(BPF_JMP | BPF_JEQ | BPF_K, cmds[i], n - i, 0);
        }
        stmt(BPF_RET | BPF_K, eperm);
        stmt(BPF_RET | BPF_K, SECCOMP_RET_ALLOW);
    }
    // open(path, flags) / openat(dirfd, path, flags): deny write or create flags
    auto openCheck = [&](uint32_t nr, int flagsArg) {
        jump(BPF_JMP | BPF_JEQ | BPF_K, nr, 0, 5);
        stmt(BPF_LD | BPF_W | BPF_ABS, offsetof(struct seccomp_data, args) + 8 * flagsArg); // low 32 bits (little endian)
        stmt(BPF_ALU | BPF_AND | BPF_K, writeFlags);
        jump(BPF_JMP | BPF_JEQ | BPF_K, 0, 1, 0);
        stmt(BPF_RET | BPF_K, eperm);
        stmt(BPF_RET | BPF_K, SECCOMP_RET_ALLOW);
    };
#        ifdef SYS_open
    openCheck(SYS_open, 1);
    stmt(BPF_LD | BPF_W | BPF_ABS, offsetof(struct seccomp_data, nr));
#        endif
    openCheck(SYS_openat, 2);
    stmt(BPF_RET | BPF_K, SECCOMP_RET_ALLOW);
    struct sock_fprog prog = { static_cast<unsigned short>(f.size()), f.data() };
    return prctl(PR_SET_SECCOMP, SECCOMP_MODE_FILTER, &prog, 0, 0) == 0;
#    endif
}
#endif

Confinement confine(bool loadFontconfig)
{
    Confinement c;
#ifdef __linux__
#    if WITH_FONTCONFIGURATION_FONTCONFIG
    // load fontconfig's configuration and caches now; it may want to write a
    // cache, which is not possible afterwards
    if (loadFontconfig) {
        FcInit();
    }
#    else
    (void)loadFontconfig;
#    endif
    if (prctl(PR_SET_NO_NEW_PRIVS, 1, 0, 0, 0) != 0) {
        return c;
    }
    c.landlockAbi = applyLandlock();
    c.seccomp = applySeccomp();
#endif
    return c;
}

void closeInheritedDescriptors()
{
#ifdef __linux__
#    ifdef SYS_close_range
    if (syscall(SYS_close_range, 3U, ~0U, 0U) == 0) {
        return;
    }
#    endif
    long maxFd = sysconf(_SC_OPEN_MAX);
    if (maxFd < 0 || maxFd > 65536) {
        maxFd = 65536;
    }
    for (int fd = 3; fd < maxFd; ++fd) {
        close(fd);
    }
#endif
}
