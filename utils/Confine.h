//========================================================================
//
// Confine.h
//
// DocSlurp: the no-write, no-network sandbox that pdfregions and pdfpagefacts apply to
// themselves once the PDF is open.
//
// This file is licensed under the GPLv2 or later
//
// Copyright (C) 2026 DocSlurp
//
//========================================================================

#ifndef CONFINE_H
#define CONFINE_H

// These tools never write a file and never need the network or another program, but they
// parse hostile documents. Once the PDF is open (and fontconfig has loaded its configuration
// and caches), confine() drops the ability to do any of that, so a parser exploit cannot
// rewrite another program's output or reach out:
//  - PR_SET_NO_NEW_PRIVS;
//  - Landlock: every filesystem write right is handled with no rule granting it (no
//    writing, creating, removing, renaming or truncating files anywhere; reads stay allowed
//    for fonts), TCP bind/connect denied, and abstract unix sockets and signals scoped to
//    this process;
//  - seccomp: execve/execveat, socket, ptrace, process_vm_readv/writev and open/openat with
//    write or create flags fail with EPERM (openat2, whose flags a filter cannot read, and
//    io_uring with ENOSYS), and so do the calls that change a file without opening it for
//    writing (chmod, chown, utimes and their variants, truncate, ftruncate, fallocate,
//    extended attributes, unlink, rename, mkdir, rmdir, link, symlink, mknod), signals to any
//    other process (kill and tgkill only to this one; tkill, sigqueue and pidfds never),
//    System V and POSIX IPC and memfd_create; calls that act on a process by pid (prlimit64,
//    setpriority, sched_setaffinity/setscheduler/setparam/setattr, ioprio_set,
//    migrate_pages, move_pages) only on this one (pid 0 or its own); setsid, setpgid,
//    unshare, setns, keyctl, add_key, request_key, flock, fork and vfork; clone without
//    CLONE_THREAD (threads still work; clone3, whose flags a filter cannot read, fails with
//    ENOSYS and the C library falls back to clone); and every ioctl but TCGETS, TIOCGWINSZ,
//    FIONREAD, FIONBIO, FIOCLEX and FIONCLEX. It is a second layer that also holds on kernels
//    without Landlock.
// Which guarantees need which kernel: seccomp (Linux 3.5+) gives everything in its list on any
// supported kernel. Landlock adds path-based write denial (ABI 1, Linux 5.13), cross-directory
// links and renames (ABI 2), truncation (ABI 3; seccomp denies truncate and ftruncate anyway),
// TCP (ABI 4; seccomp denies socket anyway), device ioctls (ABI 5) and signal and abstract
// socket scoping (ABI 6; seccomp covers signals and sockets). -confine require needs both
// Landlock (any ABI) and seccomp, and fails closed without either.
// Already open descriptors (stdout, a saved stderr) keep working, but cannot be truncated.
// Linux only; elsewhere nothing is applied.
struct Confinement
{
    int landlockAbi = 0; // 0: not applied
    bool seccomp = false;
    bool confined() const { return landlockAbi > 0 && seccomp; }
};

// <loadFontconfig>: load fontconfig's configuration and caches first (it may want to write a
// cache, which is not possible afterwards); a tool that renders text needs it, one that only
// reads the document (pdfpagefacts) saves the ~8 ms and ~2.5 MiB it costs.
Confinement confine(bool loadFontconfig = true);

// Closes every descriptor above 2, first thing in main: these tools take their input on the
// command line or stdin and write only stdout and stderr, so a descriptor a parent leaked to
// them (a writable facts or output file, say) would only be a way for an exploited parser to
// write where it must not; confinement does not stop writes to descriptors already open.
// Linux only.
void closeInheritedDescriptors();

#endif
