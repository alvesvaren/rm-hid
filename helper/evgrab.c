/*
 * evgrab - Exclusively grab an evdev device and stream events to stdout.
 *
 * Uses EVIOCGRAB to prevent other readers (like xochitl) from seeing events.
 * When this process exits (SSH disconnect, signal, etc.), the kernel
 * automatically releases the grab and the UI resumes normal input.
 *
 * Cross-compiled for ARM and embedded in the rm-mouse host binary.
 * Uploaded to /tmp on the reMarkable at runtime.
 */

#include <errno.h>
#include <fcntl.h>
#include <linux/input.h>
#include <stdio.h>
#include <string.h>
#include <sys/ioctl.h>
#include <unistd.h>

int main(int argc, char **argv)
{
    if (argc < 2) {
        fprintf(stderr, "Usage: evgrab <device>\n");
        return 1;
    }

    int fd = open(argv[1], O_RDONLY);
    if (fd < 0) {
        fprintf(stderr, "evgrab: open(%s): %s\n", argv[1], strerror(errno));
        return 1;
    }

    if (ioctl(fd, EVIOCGRAB, 1) != 0) {
        fprintf(stderr, "evgrab: EVIOCGRAB(%s): %s\n", argv[1], strerror(errno));
        close(fd);
        return 1;
    }

    fprintf(stderr, "evgrab: grabbing %s (fd=%d)\n", argv[1], fd);

    char buf[4096];
    ssize_t n;

    while ((n = read(fd, buf, sizeof(buf))) > 0) {
        const char *p = buf;
        ssize_t remaining = n;

        while (remaining > 0) {
            ssize_t written = write(STDOUT_FILENO, p, remaining);
            if (written <= 0) {
                fprintf(stderr, "evgrab: write failed: %s\n", strerror(errno));
                close(fd);
                return 1;
            }
            p += written;
            remaining -= written;
        }
    }

    if (n < 0) {
        fprintf(stderr, "evgrab: read(%s): %s\n", argv[1], strerror(errno));
    } else {
        fprintf(stderr, "evgrab: read(%s): EOF\n", argv[1]);
    }

    close(fd);
    return n < 0 ? 1 : 0;
}
