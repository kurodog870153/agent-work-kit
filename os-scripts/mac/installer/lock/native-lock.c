#include <errno.h>
#include <fcntl.h>
#include <stdio.h>
#include <string.h>
#include <sys/file.h>
#include <sys/stat.h>
#include <unistd.h>

/* Keep the flock descriptor open across exec so the installer itself owns the lock. */
int main(int argc, char **argv) {
    if (argc < 3) {
        fputs("Usage: work-install-lock LOCK_FILE COMMAND [ARG...]\n", stderr);
        return 2;
    }

    int fd = open(argv[1], O_CREAT | O_RDWR | O_NOFOLLOW, 0666);
    if (fd < 0) {
        fprintf(stderr, "Error: cannot open installation lock: %s\n", strerror(errno));
        return 1;
    }
    struct stat info;
    if (fstat(fd, &info) != 0 || !S_ISREG(info.st_mode) || info.st_nlink != 1) {
        fputs("Error: installation lock is not a regular single-link file.\n", stderr);
        close(fd);
        return 1;
    }
    /* A shared lock path must remain usable by installers from other user sessions. */
    if (info.st_uid == getuid() && (info.st_mode & 0666) != 0666) {
        if (fchmod(fd, 0666) != 0) {
            fprintf(stderr, "Error: cannot share installation lock: %s\n", strerror(errno));
            close(fd);
            return 1;
        }
    }
    if ((info.st_mode & 0111) != 0) {
        fputs("Error: installation lock has executable permissions.\n", stderr);
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
    execvp(argv[2], &argv[2]);
    fprintf(stderr, "Error: cannot start installation: %s\n", strerror(errno));
    close(fd);
    return 1;
}
