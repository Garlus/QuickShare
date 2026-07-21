import SwiftUI

struct SingleSelectionSegmentedControl: View {
    let segments: [String]
    @Binding var selection: Int
    @Namespace private var animation

    var body: some View {
        HStack(spacing: 2) {
            ForEach(Array(segments.enumerated()), id: \.offset) { index, title in
                Button(action: {
                    withAnimation(.spring(response: 0.26, dampingFraction: 0.8, blendDuration: 0)) {
                        selection = index
                    }
                }) {
                    Text(title)
                        .font(.system(size: 11, weight: .medium))
                        .foregroundColor(selection == index ? .primary : .secondary)
                        .padding(.horizontal, 14)
                        .padding(.vertical, 5)
                        .background {
                            if selection == index {
                                Capsule()
                                    .fill(.thickMaterial)
                                    .shadow(color: Color.black.opacity(0.06), radius: 1, x: 0, y: 1)
                                    .matchedGeometryEffect(id: "active_pill", in: animation)
                            }
                        }
                }
                .buttonStyle(.plain)
            }
        }
        .padding(2)
        .background {
            Capsule()
                .fill(.ultraThinMaterial)
        }
        .overlay {
            Capsule()
                .strokeBorder(Color.primary.opacity(0.12), lineWidth: 0.5)
        }
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
