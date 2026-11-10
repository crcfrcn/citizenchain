// GTK progress window for both the downloadable installer and desktop updates.
#include <gtk/gtk.h>
#include <gio/gio.h>
#include <glib/gstdio.h>
#include <errno.h>
#include <fcntl.h>
#include <signal.h>
#include <stdlib.h>
#include <sys/file.h>
#include <sys/stat.h>
#include <unistd.h>

#define APP "/usr/bin/citizenchain"
#define PACKAGE "citizenchain"

typedef struct {
    const char *package;
    const char *state;
    pid_t pid;
    GtkWidget *window, *status, *progress, *percent, *version, *cancel;
    gint phase;
    int result;
} Installer;

enum { INSTALLING, CANCELLED, FINISHED };

typedef struct {
    Installer *installer;
    int percent;
    char *message;
} Progress;

static gboolean exit_ui(gpointer unused) {
    (void)unused;
    gtk_main_quit();
    return G_SOURCE_REMOVE;
}

static gboolean render_progress(gpointer data) {
    Progress *update = data;
    Installer *installer = update->installer;
    gboolean done = update->percent < 0 || update->percent == 100;
    if (done || g_atomic_int_get(&installer->phase) != CANCELLED) {
        gtk_label_set_text(GTK_LABEL(installer->status), update->message);
    }
    if (update->percent >= 0 && (done || g_atomic_int_get(&installer->phase) != CANCELLED)) {
        char *label = g_strdup_printf("%d%%", update->percent);
        gtk_progress_bar_set_fraction(GTK_PROGRESS_BAR(installer->progress), update->percent / 100.0);
        gtk_label_set_text(GTK_LABEL(installer->percent), label);
        g_free(label);
    }
    if (done) {
        gtk_widget_set_sensitive(installer->cancel, FALSE);
        g_timeout_add(500, exit_ui, NULL);
    }
    g_free(update->message);
    g_free(update);
    return G_SOURCE_REMOVE;
}

static void show(Installer *installer, int percent, const char *message) {
    Progress *update = g_new(Progress, 1);
    *update = (Progress){installer, percent, g_strdup(message)};
    g_idle_add(render_progress, update);
}

static void set_state(Installer *installer, const char *name, const char *message) {
    if (!installer->state) return;
    char *file = g_build_filename(installer->state, name, NULL);
    if (message) g_file_set_contents(file, message, -1, NULL);
    else g_unlink(file);
    g_free(file);
}

static char *package_field(const char *package, const char *field) {
    GSubprocess *command = g_subprocess_new(G_SUBPROCESS_FLAGS_STDOUT_PIPE | G_SUBPROCESS_FLAGS_STDERR_SILENCE,
        NULL, "/usr/bin/dpkg-deb", "-f", package, field, NULL);
    if (!command) return NULL;
    char *value = NULL;
    if (!g_subprocess_communicate_utf8(command, NULL, NULL, &value, NULL, NULL) ||
        !g_subprocess_get_successful(command) || !value || !*g_strstrip(value)) {
        g_clear_pointer(&value, g_free);
    }
    g_object_unref(command);
    return value;
}

static gboolean render_version(gpointer data) {
    Progress *version = data;
    gtk_label_set_text(GTK_LABEL(version->installer->version), version->message);
    g_free(version->message);
    g_free(version);
    return G_SOURCE_REMOVE;
}

static gboolean wait_for_app(Installer *installer) {
    pid_t handoff_pid = installer->pid;
    GDir *proc = g_dir_open("/proc", 0, NULL);
    if (!proc) return FALSE;
    GArray *running = g_array_new(FALSE, FALSE, sizeof(pid_t));
    const char *entry;
    while (g_atomic_int_get(&installer->phase) != CANCELLED && (entry = g_dir_read_name(proc))) {
        char *end;
        long id = strtol(entry, &end, 10);
        if (id <= 1 || id > G_MAXINT || *end) continue;
        char path[64], exe[256];
        struct stat info;
        g_snprintf(path, sizeof path, "/proc/%ld", id);
        if (stat(path, &info) != 0 || info.st_uid != getuid()) continue;
        g_snprintf(path, sizeof path, "/proc/%ld/exe", id);
        ssize_t length = readlink(path, exe, sizeof exe - 1);
        if (length <= 0) continue;
        exe[length] = '\0';
        if (!g_str_equal(exe, APP) && !g_str_equal(exe, APP " (deleted)")) continue;
        pid_t pid = (pid_t)id;
        if (pid != handoff_pid) kill(pid, SIGTERM);
        g_array_append_val(running, pid);
    }
    g_dir_close(proc);
    if (handoff_pid > 1) g_array_append_val(running, handoff_pid);

    gint64 deadline = g_get_monotonic_time() + 45 * G_TIME_SPAN_SECOND;
    while (running->len && g_atomic_int_get(&installer->phase) != CANCELLED && g_get_monotonic_time() < deadline) {
        for (guint i = running->len; i > 0; --i) {
            pid_t pid = g_array_index(running, pid_t, i - 1);
            if (kill(pid, 0) != 0 && errno == ESRCH) g_array_remove_index_fast(running, i - 1);
        }
        if (running->len) g_usleep(150000);
    }
    gboolean exited = running->len == 0;
    g_array_unref(running);
    return exited;
}

static gboolean install_package(Installer *installer) {
    GSubprocess *command = g_subprocess_new(G_SUBPROCESS_FLAGS_STDOUT_PIPE | G_SUBPROCESS_FLAGS_STDERR_MERGE,
        NULL, "/usr/bin/pkexec", "/usr/bin/apt-get", "-y", "--no-remove", "--reinstall",
        "-o", "APT::Status-Fd=1", "install", installer->package, NULL);
    if (!command) return FALSE;
    GDataInputStream *stream = g_data_input_stream_new(g_subprocess_get_stdout_pipe(command));
    int last = 45;
    char *line;
    while ((line = g_data_input_stream_read_line(stream, NULL, NULL, NULL))) {
        if (g_str_has_prefix(line, "pmstatus:")) {
            char **parts = g_strsplit(line, ":", 4);
            if (g_strv_length(parts) == 4) {
                double fraction = g_ascii_strtod(parts[2], NULL);
                if (fraction >= 0 && fraction <= 100) {
                    int percent = 45 + (int)(fraction * 0.44);
                    if (percent > last) {
                        show(installer, percent, "正在安装公民链");
                        last = percent;
                    }
                }
            }
            g_strfreev(parts);
        }
        g_free(line);
    }
    g_object_unref(stream);
    gboolean success = g_subprocess_wait_check(command, NULL, NULL);
    g_object_unref(command);
    return success;
}

static gpointer install(gpointer data) {
    Installer *installer = data;
    const char *error = "安装被取消或失败，请重试。";
    g_autofree char *name = NULL;
    g_autofree char *number = NULL;
    char *lock_path = g_build_filename(g_get_user_runtime_dir(), "citizenchain-install.lock", NULL);
    int lock = g_open(lock_path, O_CREAT | O_RDWR | O_NOFOLLOW | O_CLOEXEC, 0600);
    g_free(lock_path);
    if (lock < 0 || flock(lock, LOCK_EX | LOCK_NB) != 0) {
        error = "公民链正在安装，或无法取得安装锁。";
        goto failed;
    }
    show(installer, 10, "正在验证安装包");
    struct stat info;
    if (!g_path_is_absolute(installer->package) || !g_str_has_suffix(installer->package, ".deb") ||
        lstat(installer->package, &info) != 0 || !S_ISREG(info.st_mode)) {
        error = "公民链安装包无效。";
        goto failed;
    }
    name = package_field(installer->package, "Package");
    number = package_field(installer->package, "Version");
    if (g_strcmp0(name, PACKAGE) != 0 || !number) {
        error = "公民链安装包无效。";
        goto failed;
    }
    Progress *version = g_new(Progress, 1);
    *version = (Progress){.installer = installer, .message = g_strdup_printf("v%s", number)};
    g_idle_add(render_version, version);
    if (g_atomic_int_get(&installer->phase) == CANCELLED) goto failed;
    set_state(installer, "started", "ready");
    show(installer, 25, "正在等待公民链退出");
    if (!wait_for_app(installer)) {
        error = "公民链未退出，安装已停止。";
        goto failed;
    }
    if (g_atomic_int_get(&installer->phase) == CANCELLED) goto failed;
    show(installer, 45, "正在安装公民链");
    // Do not interrupt apt once it has started changing installed files.
    if (!install_package(installer)) goto failed;
    if (!g_atomic_int_compare_and_exchange(&installer->phase, INSTALLING, FINISHED)) goto failed;
    show(installer, 90, "正在打开公民链");
    char *app[] = {APP, NULL};
    if (!g_spawn_async(NULL, app, NULL, G_SPAWN_STDOUT_TO_DEV_NULL | G_SPAWN_STDERR_TO_DEV_NULL, NULL, NULL, NULL, NULL)) {
        error = "安装已完成，公民链未能打开。";
        goto failed;
    }
    if (installer->state) {
        set_state(installer, "ready.json", NULL);
        set_state(installer, "started", NULL);
        set_state(installer, "install-error.txt", NULL);
        g_unlink(installer->package);
    }
    close(lock);
    show(installer, 100, "安装成功");
    return NULL;

failed:
    if (lock >= 0) close(lock);
    gboolean cancelled = !g_atomic_int_compare_and_exchange(&installer->phase, INSTALLING, FINISHED) &&
        g_atomic_int_get(&installer->phase) == CANCELLED;
    if (cancelled) error = "已取消安装";
    installer->result = cancelled ? 1 : 2;
    set_state(installer, "install-error.txt", error);
    show(installer, -1, error);
    return NULL;
}

static void cancel_install(GtkWidget *button, gpointer data) {
    (void)button;
    Installer *installer = data;
    if (g_atomic_int_compare_and_exchange(&installer->phase, INSTALLING, CANCELLED)) {
        gtk_widget_set_sensitive(installer->cancel, FALSE);
        gtk_label_set_text(GTK_LABEL(installer->status), "正在取消安装");
    }
}

static gboolean close_window(GtkWidget *window, GdkEvent *event, gpointer data) {
    (void)window;
    (void)event;
    cancel_install(NULL, data);
    return TRUE;
}

int main(int argc, char **argv) {
    Installer installer = {0};
    if (!(argc % 2)) return 2;
    for (int i = 1; i + 1 < argc; i += 2) {
        if (g_str_equal(argv[i], "--package")) installer.package = argv[i + 1];
        else if (g_str_equal(argv[i], "--state")) installer.state = argv[i + 1];
        else if (g_str_equal(argv[i], "--pid")) installer.pid = (pid_t)g_ascii_strtoll(argv[i + 1], NULL, 10);
        else return 2;
    }
    if (!installer.package || getuid() == 0 || !gtk_init_check(NULL, NULL)) return 2;

    installer.window = gtk_window_new(GTK_WINDOW_TOPLEVEL);
    gtk_window_set_title(GTK_WINDOW(installer.window), "公民链安装程序");
    gtk_window_set_default_size(GTK_WINDOW(installer.window), 388, 220);
    gtk_window_set_position(GTK_WINDOW(installer.window), GTK_WIN_POS_CENTER);
    gtk_window_set_resizable(GTK_WINDOW(installer.window), FALSE);
    g_signal_connect(installer.window, "delete-event", G_CALLBACK(close_window), &installer);
    g_signal_connect(installer.window, "destroy", G_CALLBACK(gtk_main_quit), NULL);
    GtkWidget *header = gtk_header_bar_new();
    gtk_header_bar_set_title(GTK_HEADER_BAR(header), "公民链安装程序");
    gtk_header_bar_set_show_close_button(GTK_HEADER_BAR(header), FALSE);
    installer.cancel = gtk_button_new_from_icon_name("window-close-symbolic", GTK_ICON_SIZE_BUTTON);
    gtk_widget_set_tooltip_text(installer.cancel, "取消安装");
    atk_object_set_name(gtk_widget_get_accessible(installer.cancel), "取消安装");
    gtk_header_bar_pack_end(GTK_HEADER_BAR(header), installer.cancel);
    gtk_window_set_titlebar(GTK_WINDOW(installer.window), header);
    g_signal_connect(installer.cancel, "clicked", G_CALLBACK(cancel_install), &installer);

    GtkWidget *content = gtk_box_new(GTK_ORIENTATION_VERTICAL, 12);
    gtk_container_set_border_width(GTK_CONTAINER(content), 24);
    gtk_container_add(GTK_CONTAINER(installer.window), content);
    char *directory = g_path_get_dirname(argv[0]);
    char *icon = g_build_filename(directory, "icon.png", NULL);
    gtk_window_set_icon_from_file(GTK_WINDOW(installer.window), icon, NULL);
    GdkPixbuf *logo = gdk_pixbuf_new_from_file_at_scale(icon, 56, 56, TRUE, NULL);
    if (logo) {
        gtk_box_pack_start(GTK_BOX(content), gtk_image_new_from_pixbuf(logo), FALSE, FALSE, 0);
        g_object_unref(logo);
    }
    g_free(icon);
    g_free(directory);
    GtkWidget *title = gtk_label_new(NULL);
    gtk_label_set_markup(GTK_LABEL(title), "<span size='x-large' weight='bold'>公民链</span>");
    installer.version = gtk_label_new("");
    gtk_style_context_add_class(gtk_widget_get_style_context(installer.version), "dim-label");
    gtk_widget_set_valign(title, GTK_ALIGN_BASELINE);
    gtk_widget_set_valign(installer.version, GTK_ALIGN_BASELINE);
    GtkWidget *heading = gtk_grid_new();
    gtk_grid_set_column_spacing(GTK_GRID(heading), 8);
    gtk_widget_set_halign(heading, GTK_ALIGN_CENTER);
    // Balance the version on the right so the title stays centered under the icon.
    GtkWidget *spacer = gtk_label_new(NULL);
    GtkSizeGroup *sides = gtk_size_group_new(GTK_SIZE_GROUP_HORIZONTAL);
    gtk_size_group_add_widget(sides, spacer);
    gtk_size_group_add_widget(sides, installer.version);
    g_object_unref(sides);
    gtk_grid_attach(GTK_GRID(heading), spacer, 0, 0, 1, 1);
    gtk_grid_attach(GTK_GRID(heading), title, 1, 0, 1, 1);
    gtk_grid_attach(GTK_GRID(heading), installer.version, 2, 0, 1, 1);
    gtk_box_pack_start(GTK_BOX(content), heading, FALSE, FALSE, 0);
    installer.status = gtk_label_new("正在准备安装");
    installer.progress = gtk_progress_bar_new();
    installer.percent = gtk_label_new("0%");
    gtk_box_pack_start(GTK_BOX(content), installer.status, FALSE, FALSE, 0);
    gtk_box_pack_start(GTK_BOX(content), installer.progress, FALSE, FALSE, 0);
    gtk_box_pack_start(GTK_BOX(content), installer.percent, FALSE, FALSE, 0);
    gtk_widget_show_all(installer.window);
    GThread *worker = g_thread_new("citizenchain-install", install, &installer);
    gtk_main();
    g_thread_join(worker);
    return installer.result;
}
