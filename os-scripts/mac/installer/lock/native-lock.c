#define _DARWIN_C_SOURCE
#include <errno.h>
#include <fcntl.h>
#include <stdio.h>
#include <string.h>
#include <sys/file.h>
#include <sys/stat.h>
#include <unistd.h>

/* Keep the UID-scoped flock descriptor open across exec. Never unlink the
 * lock file: concurrent openers must always refer to the same inode. */
int main(int argc, char **argv) {
    if (argc < 2) {
        fputs("Usage: work-install-lock COMMAND [ARG...]\n", stderr);
        return 2;
    }
    char lock[128];
    int n = snprintf(lock, sizeof(lock), "/private/tmp/agent-work-kit-user-%lu.lock", (unsigned long)getuid());
    if (n < 0 || (size_t)n >= sizeof(lock)) return 1;

    int fd = open(lock, O_CREAT | O_RDWR | O_NOFOLLOW, 0600);
    if (fd < 0) {
        fprintf(stderr, "Error: cannot open installation lock: %s\n", strerror(errno));
        return 1;
    }
    struct stat info;
    if (fstat(fd, &info) != 0 || !S_ISREG(info.st_mode) || info.st_nlink != 1 ||
        info.st_uid != getuid() || (info.st_mode & 07777) != 0600) {
        fputs("Error: unsafe installation lock file.\n", stderr);
        close(fd);
        return 1;
    }
    if (flock(fd, LOCK_EX | LOCK_NB) != 0) {
        if (errno == EWOULDBLOCK) {
            fputs("Error: another Work installation is running.\n", stderr);
        } else {
            fprintf(stderr, "Error: cannot acquire installation lock: %s\n", strerror(errno));
        }
        close(fd);
        return 1;
    }
    int flags = fcntl(fd, F_GETFD);
    if (flags < 0 || fcntl(fd, F_SETFD, flags & ~FD_CLOEXEC) != 0) {
        fprintf(stderr, "Error: cannot retain installation lock: %s\n", strerror(errno));
        close(fd);
        return 1;
    }
    execvp(argv[1], &argv[1]);
    fprintf(stderr, "Error: cannot start installation: %s\n", strerror(errno));
    close(fd);
    return 1;
}
