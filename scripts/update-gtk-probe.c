// Qualification-only GTK module. Observe the native focused control without
// changing focus, defaults, responses or input. AT-SPI omits some rfd dialogs.
// Resolve against the AppImage's GTK, rather than linking a second GTK version.
#include <dlfcn.h>
#include <stdint.h>
#include <string.h>
#include <unistd.h>

typedef struct List { void *data; struct List *next, *prev; } List;
#define API(result, name, args) static result (*name) args
API(List *, gtk_window_list_toplevels, (void));
API(const char *, gtk_window_get_title, (void *));
API(void *, gtk_window_get_focus, (void *));
API(int, gtk_widget_get_visible, (void *));
API(void *, gtk_widget_get_window, (void *));
API(uintptr_t, gtk_dialog_get_type, (void));
API(uintptr_t, gtk_container_get_type, (void));
API(uintptr_t, gtk_button_get_type, (void));
API(int, g_type_check_instance_is_a, (void *, uintptr_t));
API(List *, gtk_container_get_children, (void *));
API(const char *, gtk_button_get_label, (void *));
API(int, gtk_widget_is_sensitive, (void *));
API(int, gtk_dialog_get_response_for_widget, (void *, void *));
API(void, g_list_free, (List *));
API(void *, gdk_atom_intern, (const char *, int));
API(void, gdk_property_change, (void *, void *, void *, int, int, const unsigned char *, int));
API(unsigned int, g_timeout_add, (unsigned int, int (*)(void *), void *));

static unsigned long serial;
struct Scan { void *dialog, *focus; unsigned long nodes, installs, focused, ordinal; int valid; };

static void scan(void *widget, struct Scan *s, unsigned int depth) {
    if (++s->nodes > 128 || depth > 16) { s->valid = 0; return; }
    if (widget == s->focus) s->ordinal = s->nodes;
    if (g_type_check_instance_is_a(widget, gtk_button_get_type())) {
        const char *label = gtk_button_get_label(widget);
        int install = label && !strcmp(label, "Install and Restart") &&
            gtk_widget_get_visible(widget) && gtk_widget_is_sensitive(widget) &&
            gtk_dialog_get_response_for_widget(s->dialog, widget) != -1;
        if (install) ++s->installs;
        if (widget == s->focus) s->focused = install ? 1 : 2;
    }
    if (!g_type_check_instance_is_a(widget, gtk_container_get_type())) return;
    List *children = gtk_container_get_children(widget);
    for (List *item = children; item && s->valid; item = item->next)
        scan(item->data, s, depth + 1);
    g_list_free(children);
}

static int observe(void *unused) {
    (void)unused;
    List *windows = gtk_window_list_toplevels();
    unsigned int count = 0;
    ++serial;
    for (List *item = windows; item && ++count <= 20; item = item->next) {
        void *dialog = item->data;
        if (!gtk_widget_get_visible(dialog) ||
            !g_type_check_instance_is_a(dialog, gtk_dialog_get_type())) continue;
        const char *title = gtk_window_get_title(dialog);
        if (!title || strcmp(title, "An update is available")) continue;
        void *window = gtk_widget_get_window(dialog);
        if (!window) continue;
        struct Scan s = { .dialog = dialog, .focus = gtk_window_get_focus(dialog), .valid = 1 };
        scan(dialog, &s, 0);
        // Only fixed facts, never captions, user text or addresses. Format 32
        // GDK properties use native unsigned-long slots (as XChangeProperty).
        unsigned long facts[] = { 1, (unsigned long)getpid(), s.installs,
            s.focused, s.ordinal, serial & 0xffffffffUL, (unsigned long)s.valid };
        gdk_property_change(window, gdk_atom_intern("_IBIS_UPDATE_INSTALL_CONTROL", 0),
            gdk_atom_intern("CARDINAL", 0), 32, 0, (const unsigned char *)facts, 7);
    }
    g_list_free(windows);
    return 1;
}

void gtk_module_init(int *argc, char ***argv) {
    (void)argc; (void)argv;
#define LOAD(name) do { *(void **)(&name) = dlsym(RTLD_DEFAULT, #name); if (!name) return; } while (0)
    LOAD(gtk_window_list_toplevels); LOAD(gtk_window_get_title); LOAD(gtk_window_get_focus);
    LOAD(gtk_widget_get_visible); LOAD(gtk_widget_get_window); LOAD(gtk_dialog_get_type);
    LOAD(gtk_container_get_type); LOAD(gtk_button_get_type); LOAD(g_type_check_instance_is_a);
    LOAD(gtk_container_get_children); LOAD(gtk_button_get_label); LOAD(gtk_widget_is_sensitive);
    LOAD(gtk_dialog_get_response_for_widget); LOAD(g_list_free); LOAD(gdk_atom_intern);
    LOAD(gdk_property_change); LOAD(g_timeout_add);
#undef LOAD
    // The timer reads only this owned qualification process. No product
    // polling or production module is introduced.
    g_timeout_add(50, observe, 0);
}
