import SwiftUI

struct SingleSelectionSegmentedControl: View {
    let segments: [String]
    @Binding var selection: Int

    var body: some View {
        HStack(spacing: 0) {
            ForEach(Array(segments.enumerated()), id: \.offset) { index, title in
                Button(action: { withAnimation(.easeInOut(duration: 0.2)) { selection = index } }) {
                    Text(title)
                        .font(.system(size: 12, weight: .medium))
                        .foregroundColor(selection == index ? .primary : .secondary)
                        .padding(.horizontal, 14)
                        .padding(.vertical, 5)
                        .background(
                            Capsule()
                                .fill(selection == index
                                      ? Color.primary.opacity(0.12)
                                      : Color.clear)
                        )
                        .contentShape(Capsule())
                }
                .buttonStyle(.plain)
            }
        }
        .background(
            Capsule()
                .strokeBorder(Color.primary.opacity(0.1), lineWidth: 0.5)
        )
    }
}

// MARK: - Bool Convenience

extension SingleSelectionSegmentedControl {
    init(_ labels: [String], selection: Binding<Bool>) {
        self.init(
            segments: labels,
            selection: Binding(
                get: { selection.wrappedValue ? 1 : 0 },
                set: { selection.wrappedValue = $0 == 1 }
            )
        )
    }
}
