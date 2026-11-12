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
#    include <sys/prctl.h>
#    include <sys/syscall.h>
#endif
#if WITH_FONTCONFIGURATION_FONTCONFIG
#    include <fontconfig/fontconfig.h>
#endif


#ifdef __linux__
static int applyLandlock()
{
#    if defined(SYS_landlock_create_ruleset) && defined(SYS_landlock_restrict_self)
    const long abi = syscall(SYS_landlock_create_ruleset, nullptr, 0, LANDLOCK_CREATE_RULESET_VERSION);
    if (abi < 1) {
        return 0;
    }
    struct landlock_ruleset_attr attr;
    memset(&attr, 0, sizeof(attr));
    attr.handled_access_fs = LANDLOCK_ACCESS_FS_EXECUTE | LANDLOCK_ACCESS_FS_WRITE_FILE | LANDLOCK_ACCESS_FS_REMOVE_DIR | LANDLOCK_ACCESS_FS_REMOVE_FILE | LANDLOCK_ACCESS_FS_MAKE_CHAR | LANDLOCK_ACCESS_FS_MAKE_DIR | LANDLOCK_ACCESS_FS_MAKE_REG
            | LANDLOCK_ACCESS_FS_MAKE_SOCK | LANDLOCK_ACCESS_FS_MAKE_FIFO | LANDLOCK_ACCESS_FS_MAKE_BLOCK | LANDLOCK_ACCESS_FS_MAKE_SYM;
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
    const uint32_t denied[] = {
        SYS_execve, SYS_execveat, SYS_socket, SYS_socketpair, SYS_connect, SYS_ptrace, SYS_process_vm_readv, SYS_process_vm_writev,
#        ifdef SYS_creat
        SYS_creat,
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

Confinement confine()
{
    Confinement c;
#ifdef __linux__
#    if WITH_FONTCONFIGURATION_FONTCONFIG
    // load fontconfig's configuration and caches now; it may want to write a
    // cache, which is not possible afterwards
    FcInit();
#    endif
    if (prctl(PR_SET_NO_NEW_PRIVS, 1, 0, 0, 0) != 0) {
        return c;
    }
    c.landlockAbi = applyLandlock();
    c.seccomp = applySeccomp();
#endif
    return c;
}
