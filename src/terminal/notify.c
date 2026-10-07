/* SPDX-License-Identifier: MIT */
#include "terminal/notify.h"

#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include <strings.h>
#include <unistd.h>

#include "config.h"
#include "terminal/ansi.h"
#include "text/utf8.h"

enum notify_method {
    NOTIFY_METHOD_DISABLED,
    NOTIFY_METHOD_BEL,
    NOTIFY_METHOD_OSC9,
};

/* Kitty consumes unsupported OSC 9, including its BEL terminator, so detect support positively. */
static int terminal_supports_osc9(void)
{
    static const char *const TERM_PROGRAMS[] = {"iTerm.app", "ghostty", "WezTerm", "WarpTerminal"};
    const char *term_program = getenv("TERM_PROGRAM");
    if (term_program) {
        for (size_t i = 0; i < sizeof(TERM_PROGRAMS) / sizeof(TERM_PROGRAMS[0]); i++) {
            if (strcmp(term_program, TERM_PROGRAMS[i]) == 0)
                return 1;
        }
    }
    if (getenv("GHOSTTY_RESOURCES_DIR") || getenv("GHOSTTY_BIN_DIR"))
        return 1;
    if (getenv("WEZTERM_EXECUTABLE") || getenv("WEZTERM_PANE"))
        return 1;
    return 0;
}

static enum notify_method select_notify_method(void)
{
    if (!isatty(STDOUT_FILENO))
        return NOTIFY_METHOD_DISABLED;

    const char *configured_method = config_str("notify");
    if (configured_method && strcasecmp(configured_method, "off") == 0)
        return NOTIFY_METHOD_DISABLED;
    if (configured_method && strcasecmp(configured_method, "bel") == 0)
        return NOTIFY_METHOD_BEL;
    if (configured_method && strcasecmp(configured_method, "osc9") == 0)
        return NOTIFY_METHOD_OSC9;

    const char *terminal_type = getenv("TERM");
    if (terminal_type && strcmp(terminal_type, "dumb") == 0)
        return NOTIFY_METHOD_DISABLED;

    /* tmux may drop DCS passthrough and offers no in-band query for allow-passthrough. */
    if (getenv("TMUX"))
        return NOTIFY_METHOD_BEL;

    return terminal_supports_osc9() ? NOTIFY_METHOD_OSC9 : NOTIFY_METHOD_BEL;
}

#define NOTIFY_MESSAGE_MAX_BYTES 240

/* OSC strings must not contain controls: model output is untrusted terminal input. */
static int emit_message(const char *message)
{
    if (!message || !*message)
        return 0;

    size_t length = strlen(message);
    size_t offset = 0;
    size_t written = 0;
    int pending_space = 0;
    while (offset < length && written < NOTIFY_MESSAGE_MAX_BYTES) {
        unsigned char byte = (unsigned char)message[offset];
        if (byte < 0x20 || byte == 0x7f) {
            pending_space |= written > 0;
            offset++;
            continue;
        }

        size_t sequence_len = utf8_sequence_length(byte);
        if (sequence_len > length - offset ||
            !utf8_sequence_is_valid(message + offset, sequence_len)) {
            offset++;
            continue;
        }
        if (written + (pending_space ? 1 : 0) + sequence_len > NOTIFY_MESSAGE_MAX_BYTES)
            break;
        if (pending_space) {
            fputc(' ', stdout);
            written++;
            pending_space = 0;
        }
        fwrite(message + offset, 1, sequence_len, stdout);
        written += sequence_len;
        offset += sequence_len;
    }
    return written > 0;
}

static void emit_osc9_notification(const char *message)
{
    int tmux_wrap = getenv("TMUX") != NULL;
    if (tmux_wrap)
        fputs(ANSI_TMUX_PASSTHROUGH_BEGIN, stdout);
    fputs(ANSI_ESC "]9;hax:", stdout);
    if (!emit_message(message))
        fputs(" ready", stdout);
    fputs(ANSI_BEL, stdout);
    if (tmux_wrap)
        fputs(ANSI_TMUX_PASSTHROUGH_END, stdout);
}

void notify_attention(const char *message)
{
    enum notify_method method = select_notify_method();

    switch (method) {
    case NOTIFY_METHOD_DISABLED:
        return;
    case NOTIFY_METHOD_BEL:
        fputs(ANSI_BEL, stdout);
        break;
    case NOTIFY_METHOD_OSC9:
        emit_osc9_notification(message);
        break;
    }
    fflush(stdout);
}
