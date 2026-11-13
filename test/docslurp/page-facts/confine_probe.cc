// confine_probe: applies confine() (utils/Confine.cc) as pdfpagefacts and pdfregions do, then
// tries what the confinement must deny and prints one "name=denied|allowed|errno" per line.
// usage: docslurp-confine-probe DIR  (DIR holds a writable file "f"; the probe changes only DIR)
#include "Confine.h"

#include <cerrno>
#include <cstdio>
#include <cstring>
#include <string>
#include <fcntl.h>
#include <linux/fs.h>
#include <pthread.h>
#include <sched.h>
#include <signal.h>
#include <sys/file.h>
#include <sys/ioctl.h>
#include <sys/resource.h>
#include <sys/wait.h>
#include <sys/ipc.h>
#include <sys/msg.h>
#include <sys/sem.h>
#include <sys/shm.h>
#include <sys/stat.h>
#include <sys/syscall.h>
#include <sys/time.h>
#include <sys/xattr.h>
#include <unistd.h>
#include <utime.h>

static void report(const char *name, long rc)
{
    if (rc >= 0) {
        printf("%s=allowed\n", name);
    } else if (errno == EPERM || errno == EACCES) {
        printf("%s=denied\n", name);
    } else {
        printf("%s=errno-%d\n", name, errno);
    }
}

#define PROBE(name, expr)                                                                                                                                                                                                                      \
    do {                                                                                                                                                                                                                                       \
        errno = 0;                                                                                                                                                                                                                             \
        report(name, static_cast<long>(expr));                                                                                                                                                                                                 \
    } while (0)

int main(int argc, char **argv)
{
    if (argc != 2) {
        return 2;
    }
    const std::string dir = argv[1];
    const std::string f = dir + "/f";
    const int rfd = open(f.c_str(), O_RDONLY);
    const int wfd = open(f.c_str(), O_WRONLY); // like stdout: open before confinement
    if (rfd < 0 || wfd < 0) {
        return 2;
    }
    const pid_t parent = getppid();
    // values read before confinement, so that a call that is wrongly allowed changes nothing
    const int parentNice = getpriority(PRIO_PROCESS, parent);
    const long parentIoprio = syscall(SYS_ioprio_get, 1, parent);
    cpu_set_t parentCpus;
    CPU_ZERO(&parentCpus);
    sched_getaffinity(parent, sizeof(parentCpus), &parentCpus);
    int fileFlags = 0;
    ioctl(rfd, FS_IOC_GETFLAGS, &fileFlags);
    const Confinement c = confine(false);
    printf("landlock=%d seccomp=%d\n", c.landlockAbi, c.seccomp ? 1 : 0);
    // still allowed: this process, its open descriptors, reading
    PROBE("kill-self", kill(getpid(), 0));
    PROBE("write-open-fd", write(wfd, "x", 1));
    char b;
    PROBE("read", read(rfd, &b, 1));
    struct rlimit rl;
    PROBE("prlimit-self", prlimit(0, RLIMIT_NOFILE, nullptr, &rl));
    PROBE("setpriority-self", setpriority(PRIO_PROCESS, 0, getpriority(PRIO_PROCESS, 0)));
    int avail = 0;
    PROBE("ioctl-fionread", ioctl(rfd, FIONREAD, &avail));
    PROBE("ioctl-fioclex", ioctl(rfd, FIOCLEX));
    {
        pthread_t th;
        errno = 0;
        const int rc = pthread_create(&th, nullptr, [](void *) -> void * { return nullptr; }, nullptr);
        if (rc == 0) {
            pthread_join(th, nullptr);
        }
        errno = rc;
        report("thread", rc == 0 ? 0 : -1);
    }
    // clone3 is unavailable (ENOSYS), so that the C library falls back to clone
#ifdef SYS_clone3
    PROBE("clone3", syscall(SYS_clone3, nullptr, 0));
#endif
    // denied
    PROBE("chmod", chmod(f.c_str(), 0600));
    PROBE("fchmod", fchmod(rfd, 0600));
    PROBE("fchmodat", fchmodat(AT_FDCWD, f.c_str(), 0600, 0));
    PROBE("fchmodat2", syscall(452, AT_FDCWD, f.c_str(), 0600, 0));
    PROBE("chown", chown(f.c_str(), getuid(), getgid()));
    PROBE("fchown", fchown(rfd, getuid(), getgid()));
    PROBE("fchownat", fchownat(AT_FDCWD, f.c_str(), getuid(), getgid(), 0));
    PROBE("lchown", lchown(f.c_str(), getuid(), getgid()));
    PROBE("utime", utime(f.c_str(), nullptr));
    PROBE("utimes", utimes(f.c_str(), nullptr));
    PROBE("utimensat", utimensat(AT_FDCWD, f.c_str(), nullptr, 0));
    PROBE("futimens", futimens(rfd, nullptr));
#ifdef SYS_futimesat
    PROBE("futimesat", syscall(SYS_futimesat, AT_FDCWD, f.c_str(), nullptr));
#endif
    PROBE("truncate", truncate(f.c_str(), 0));
    PROBE("ftruncate", ftruncate(wfd, 0));
    PROBE("fallocate", fallocate(wfd, 0, 0, 4096));
    PROBE("setxattr", setxattr(f.c_str(), "user.probe", "1", 1, 0));
    PROBE("fsetxattr", fsetxattr(rfd, "user.probe", "1", 1, 0));
    PROBE("removexattr", removexattr(f.c_str(), "user.probe"));
    PROBE("unlink", unlink(f.c_str()));
    PROBE("rename", rename(f.c_str(), (dir + "/g").c_str()));
    PROBE("mkdir", mkdir((dir + "/d").c_str(), 0700));
    PROBE("link", link(f.c_str(), (dir + "/h").c_str()));
    PROBE("symlink", symlink(f.c_str(), (dir + "/s").c_str()));
    PROBE("mkfifo", mkfifo((dir + "/p").c_str(), 0600));
    PROBE("kill-parent", kill(parent, 0));
    PROBE("kill-group", kill(0, 0));
    PROBE("tgkill-parent", syscall(SYS_tgkill, parent, parent, 0));
    PROBE("tkill-parent", syscall(SYS_tkill, parent, 0));
#ifdef SYS_pidfd_open
    PROBE("pidfd_open", syscall(SYS_pidfd_open, parent, 0));
#endif
    // other processes by pid: resource limits, priority, scheduling, memory placement
    PROBE("prlimit-parent", prlimit(parent, RLIMIT_NOFILE, nullptr, &rl));
    PROBE("setpriority-parent", setpriority(PRIO_PROCESS, parent, parentNice));
    PROBE("setpriority-group", setpriority(PRIO_PGRP, 0, getpriority(PRIO_PGRP, 0)));
    PROBE("sched_setaffinity-parent", syscall(SYS_sched_setaffinity, parent, sizeof(parentCpus), &parentCpus));
    struct sched_param sp = {};
    PROBE("sched_setscheduler-parent", syscall(SYS_sched_setscheduler, parent, SCHED_OTHER, &sp));
    PROBE("sched_setparam-parent", syscall(SYS_sched_setparam, parent, &sp));
    PROBE("ioprio_set-parent", syscall(SYS_ioprio_set, 1, parent, parentIoprio));
    PROBE("migrate_pages-parent", syscall(SYS_migrate_pages, parent, 0, nullptr, nullptr));
    PROBE("move_pages-parent", syscall(SYS_move_pages, parent, 0, nullptr, nullptr, nullptr, 0));
    // ioctls outside the allow-list, even on a read-only descriptor
    PROBE("ioctl-setflags", ioctl(rfd, FS_IOC_SETFLAGS, &fileFlags));
    PROBE("ioctl-getflags", ioctl(rfd, FS_IOC_GETFLAGS, &fileFlags));
    // shared locks, sessions, process groups, namespaces, keyrings, new processes
    PROBE("flock", flock(rfd, LOCK_SH));
    PROBE("setsid", setsid());
    PROBE("setpgid", setpgid(0, 0));
    PROBE("unshare", unshare(0));
    PROBE("setns", setns(-1, 0));
    PROBE("keyctl", syscall(SYS_keyctl, 0, -2, 0));
    PROBE("add_key", syscall(SYS_add_key, "user", "probe", "x", 1, -2));
    PROBE("request_key", syscall(SYS_request_key, "user", "probe-none", nullptr, -2));
    {
        errno = 0;
        const long pid = syscall(SYS_clone, SIGCHLD, 0, 0, 0, 0);
        if (pid == 0) {
            _exit(0);
        }
        if (pid > 0) {
            waitpid(static_cast<pid_t>(pid), nullptr, 0);
        }
        report("clone-process", pid);
    }
#ifdef SYS_fork
    {
        errno = 0;
        const long pid = syscall(SYS_fork);
        if (pid == 0) {
            _exit(0);
        }
        if (pid > 0) {
            waitpid(static_cast<pid_t>(pid), nullptr, 0);
        }
        report("fork", pid);
    }
#endif
    const int shm = shmget(IPC_PRIVATE, 4096, IPC_CREAT | 0600);
    report("shmget", shm);
    const int msg = msgget(IPC_PRIVATE, IPC_CREAT | 0600);
    report("msgget", msg);
    const int sem = semget(IPC_PRIVATE, 1, IPC_CREAT | 0600);
    report("semget", sem);
    PROBE("memfd_create", syscall(SYS_memfd_create, "probe", 0));
    PROBE("mq_open", syscall(SYS_mq_open, "probe", O_RDWR | O_CREAT, 0600, nullptr));
    // a failure above must not leak IPC objects
    if (shm >= 0) {
        shmctl(shm, IPC_RMID, nullptr);
    }
    if (msg >= 0) {
        msgctl(msg, IPC_RMID, nullptr);
    }
    if (sem >= 0) {
        semctl(sem, 0, IPC_RMID);
    }
    return 0;
}
