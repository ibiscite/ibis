import AppKit
import SwiftUI

struct PickerView: View {
    @ObservedObject var model: PickerModel
    let chooseLibrary: () -> Void
    let deliver: () -> Void

    var body: some View {
        VStack(spacing: 0) {
            HStack {
                PickerSearchField(query: $model.query)
                if let catalog = model.catalog {
                    Picker("Format", selection: $model.format) {
                        ForEach(catalog.formats, id: \.id) { choice in
                            Text(choice.name).tag(choice.id)
                        }
                    }
                    .fixedSize()
                }
            }
            .padding()

            Divider()
            if let status = model.heading {
                Text(status)
                    .font(.callout)
                    .foregroundStyle(.secondary)
                    .frame(maxWidth: .infinity, alignment: .leading)
                    .padding(.horizontal)
                    .padding(.vertical)
            }
            results
            Divider()
            VStack {
                DisclosureGroup("Citation Details", isExpanded: $model.drawerOpen) {
                    CitationDrawer(model: model).padding(.top)
                }
                .disabled(model.selected == nil)

                HStack {
                    if let preview = model.preview {
                        Text(preview.problem ?? preview.text)
                            .foregroundStyle(preview.problem == nil ? .primary : .secondary)
                            .lineLimit(2)
                            .help(preview.problem ?? preview.text)
                    }
                    Spacer()
                    Button(model.pasteExpected ? "Paste Citation" : "Copy Citation", action: deliver)
                        .keyboardShortcut(.defaultAction)
                        .disabled(!model.canDeliver)
                }
            }
            .padding()
        }
        .disabled(model.isDelivering)
    }

    private var results: some View {
        ScrollViewReader { scroll in
            List(selection: Binding(get: { model.selected }, set: { model.select($0) })) {
                ForEach(model.rows, id: \.id) { row in
                    ResultRow(row: row)
                        .tag(row.id)
                        .id(row.id)
                }
            }
            .listStyle(.plain)
            .accessibilityLabel("References")
            .onChange(of: model.selected) { id in
                if let id { scroll.scrollTo(id) }
            }
            .overlay {
                if model.rows.isEmpty {
                    VStack {
                        if let message = model.emptyMessage {
                            Text(message).foregroundStyle(.secondary)
                        }
                        if model.catalog?.libraryOpen != true {
                            Button("Choose Library…", action: chooseLibrary)
                        }
                    }
                }
            }
        }
    }
}

private struct ResultRow: View {
    let row: PickerRow

    var body: some View {
        VStack(alignment: .leading) {
            Text(row.title ?? "Untitled")
                .lineLimit(1, reservesSpace: true)
            Text(byline)
                .font(.subheadline)
                .foregroundStyle(.secondary)
                .lineLimit(1, reservesSpace: true)
        }
        .frame(maxWidth: .infinity, alignment: .leading)
        .accessibilityElement(children: .combine)
    }

    private var byline: String {
        var parts = [row.authors.prefix(2).joined(separator: ", ")]
        if row.authors.count > 2 { parts[0] = row.authors[0] + " et al." }
        parts.append(row.year ?? "No date")
        if let journal = row.journal { parts.append(journal) }
        return parts.filter { !$0.isEmpty }.joined(separator: " · ")
    }
}

private struct CitationDrawer: View {
    @ObservedObject var model: PickerModel

    var body: some View {
        Grid(alignment: .leading) {
            GridRow {
                Text("Show as")
                Picker("Show as", selection: $model.details.form) {
                    Text("Default").tag(nil as CitationForm?)
                    ForEach(model.forms, id: \.form) { choice in
                        Text(choice.name).tag(Optional(choice.form))
                    }
                }
                .labelsHidden()
                .accessibilityLabel("Show as")
            }
            GridRow {
                Text("Locator")
                HStack {
                    Picker("Locator type", selection: $model.details.term) {
                        ForEach(model.catalog?.locators ?? [], id: \.term) { choice in
                            Text(choice.name).tag(choice.term)
                        }
                    }
                    .labelsHidden()
                    .accessibilityLabel("Locator type")
                    .fixedSize()
                    TextField("Locator value", text: $model.details.locator)
                        .accessibilityLabel("Locator value")
                }
            }
            GridRow {
                Text("Text before")
                TextField("", text: $model.details.prefix)
                    .accessibilityLabel("Text before")
            }
            GridRow {
                Text("Text after")
                TextField("", text: $model.details.suffix)
                    .accessibilityLabel("Text after")
            }
        }
    }
}

private struct PickerSearchField: NSViewRepresentable {
    @Binding var query: String

    final class Field: NSSearchField {
        override func viewDidMoveToWindow() {
            super.viewDidMoveToWindow()
            guard let panel = window as? PickerPanel else { return }
            panel.searchField = self
            panel.initialFirstResponder = self
            if panel.isKeyWindow { panel.makeFirstResponder(self) }
        }
    }

    final class Coordinator: NSObject, NSSearchFieldDelegate {
        var query: Binding<String>
        init(query: Binding<String>) { self.query = query }

        func controlTextDidChange(_ notification: Notification) {
            if let field = notification.object as? NSSearchField { query.wrappedValue = field.stringValue }
        }
    }

    func makeCoordinator() -> Coordinator { Coordinator(query: $query) }

    func makeNSView(context: Context) -> NSSearchField {
        let field = Field()
        field.placeholderString = "Search references"
        field.setAccessibilityLabel("Search references")
        field.delegate = context.coordinator
        return field
    }

    func updateNSView(_ field: NSSearchField, context: Context) {
        context.coordinator.query = $query
        if field.stringValue != query { field.stringValue = query }
    }
}
