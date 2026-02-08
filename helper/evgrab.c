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

#include <fcntl.h>
#include <linux/input.h>
#include <sys/ioctl.h>
#include <unistd.h>

int main(int argc, char **argv)
{
    if (argc < 2) {
        const char msg[] = "Usage: evgrab <device>\n";
        (void)write(STDERR_FILENO, msg, sizeof(msg) - 1);
        return 1;
    }

    int fd = open(argv[1], O_RDONLY);
    if (fd < 0)
        return 1;

    if (ioctl(fd, EVIOCGRAB, 1) != 0) {
        close(fd);
        return 1;
    }

    char buf[4096];
    ssize_t n;

    while ((n = read(fd, buf, sizeof(buf))) > 0) {
        const char *p = buf;
        ssize_t remaining = n;

        while (remaining > 0) {
            ssize_t written = write(STDOUT_FILENO, p, remaining);
            if (written <= 0) {
                close(fd);
                return 1;
            }
            p += written;
            remaining -= written;
        }
    }

    close(fd);
    return 0;
}
