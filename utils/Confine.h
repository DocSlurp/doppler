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
//    write or create flags fail with EPERM (openat2, whose flags a filter cannot read, with
//    ENOSYS), as a second layer that also holds on kernels without Landlock.
// Already open descriptors (stdout, a saved stderr) keep working. Linux only; elsewhere
// nothing is applied.
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

#endif
