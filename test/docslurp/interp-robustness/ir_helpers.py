"""Helpers shared by the interp-robustness tests and benchmarks."""

import os
import subprocess
import tempfile


class Result:
    def __init__(self, returncode, stdout, stderr, user, system, maxrss_kb, timed_out):
        self.returncode = returncode
        self.stdout = stdout
        self.stderr = stderr
        self.user = user
        self.system = system
        self.maxrss_kb = maxrss_kb
        self.timed_out = timed_out

    @property
    def cpu(self):
        return self.user + self.system

    @property
    def text(self):
        return self.stdout.decode("utf-8", errors="replace")


def measure(cmd, timeout=60):
    """Run cmd, return a Result with the child's own CPU time and peak RSS.

    Uses wait4() so the numbers belong to this child alone (RUSAGE_CHILDREN
    would only report the maximum RSS over every child run so far).  A child
    that runs past `timeout` wall-clock seconds is killed and flagged.
    """
    import signal
    import threading

    with tempfile.TemporaryFile() as out, tempfile.TemporaryFile() as err:
        proc = subprocess.Popen([str(c) for c in cmd], stdout=out, stderr=err, stdin=subprocess.DEVNULL)
        timed_out = []

        def kill():
            timed_out.append(True)
            try:
                proc.send_signal(signal.SIGKILL)
            except ProcessLookupError:
                pass

        timer = threading.Timer(timeout, kill)
        timer.start()
        _, status, ru = os.wait4(proc.pid, 0)
        timer.cancel()
        proc.returncode = os.waitstatus_to_exitcode(status)
        out.seek(0)
        err.seek(0)
        return Result(proc.returncode, out.read(), err.read(), ru.ru_utime, ru.ru_stime, ru.ru_maxrss, bool(timed_out))
