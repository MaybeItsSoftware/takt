import SwiftUI

/// A "Sign in with Google" or "Sign in with Apple" button, shared by the Mac
/// and iOS settings screens.
///
/// Each brand's own rules for colour, mark, type and wording — the one place
/// the apps set aside their theme, because the marks aren't ours to recolour.
/// The shape is Takt's: flat, at the theme's control radius, with a hairline
/// rather than a shadow.
///
/// Google (developers.google.com/identity/branding-guidelines): the
/// standard-colour "G" from Google's download kit (`GoogleG` in each app's
/// asset catalogue, rendered from the kit's SVG), Google Sans Medium 14/20
/// (`GoogleSansSignIn-Medium`, subset to the approved labels, in
/// `shared/fonts`), and the light (#FFFFFF, #747775 border, #1F1F1F text) or
/// dark (#131314, #8E918F, #E3E3E3) button.
///
/// Apple (HIG, Sign in with Apple): black with white in a light appearance,
/// white with black in a dark one, the Apple logo and the system font. The
/// iPhone uses the native `SignInWithAppleButton`; the Mac signs in through
/// Supabase's web flow, so it draws the same button with `apple.logo`.
///
/// The appearance is the theme's, not the system's: a theme locked to dark
/// gets the dark buttons on a light system.
public struct SyncProviderButton: View {
  public enum Brand: Sendable {
    case google
    case apple
  }

  /// The approved wordings. "Continue" is for signing in again to an account
  /// already connected, where "Sign in" would read as a second account.
  public enum Wording: Sendable {
    case signIn
    case `continue`
  }

  let brand: Brand
  let wording: Wording
  let isDark: Bool
  let cornerRadius: CGFloat
  let height: CGFloat
  let fillsWidth: Bool
  let action: () -> Void

  public init(
    _ brand: Brand,
    wording: Wording = .signIn,
    isDark: Bool,
    cornerRadius: CGFloat,
    height: CGFloat = 44,
    fillsWidth: Bool = true,
    action: @escaping () -> Void
  ) {
    self.brand = brand
    self.wording = wording
    self.isDark = isDark
    self.cornerRadius = cornerRadius
    self.height = height
    self.fillsWidth = fillsWidth
    self.action = action
  }

  public var body: some View {
    Button(action: action) {
      SyncProviderButtonLabel(
        brand: brand, title: title, isDark: isDark, height: height, fillsWidth: fillsWidth)
    }
    .buttonStyle(SyncProviderButtonStyle(brand: brand, isDark: isDark, cornerRadius: cornerRadius))
    .accessibilityLabel(title)
  }

  var title: String {
    switch (brand, wording) {
    case (.google, .signIn): "Sign in with Google"
    case (.google, .continue): "Continue with Google"
    case (.apple, .signIn): "Sign in with Apple"
    case (.apple, .continue): "Continue with Apple"
    }
  }
}

/// The mark and the words, at the brand's sizes.
struct SyncProviderButtonLabel: View {
  let brand: SyncProviderButton.Brand
  let title: String
  let isDark: Bool
  let height: CGFloat
  let fillsWidth: Bool

  var body: some View {
    HStack(spacing: brand == .google ? Self.googleGap : 6) {
      mark
      Text(title)
        .font(font)
        .lineLimit(1)
        .fixedSize()
    }
    .padding(.horizontal, brand == .google ? Self.googleInset : 16)
    .frame(maxWidth: fillsWidth ? .infinity : nil, minHeight: height)
  }

  @ViewBuilder
  private var mark: some View {
    switch brand {
    case .google:
      // The kit's 20pt logo, unscaled whatever the button's height.
      Image("GoogleG")
        .resizable()
        .interpolation(.high)
        .frame(width: 20, height: 20)
        .accessibilityHidden(true)
    case .apple:
      Image(systemName: "apple.logo")
        .font(.system(size: 15, weight: .medium))
        .accessibilityHidden(true)
    }
  }

  private var font: Font {
    switch brand {
    case .google: Font.custom("GoogleSansSignIn-Medium", fixedSize: 14)
    case .apple: .system(size: 15, weight: .medium)
    }
  }

  // The kit's padding: 16pt to the logo and 12pt to the words on iOS, 12 and
  // 10 on the Android and web buttons the Mac's matches.
  #if os(iOS)
    static let googleInset: CGFloat = 16
    static let googleGap: CGFloat = 12
  #else
    static let googleInset: CGFloat = 12
    static let googleGap: CGFloat = 10
  #endif
}

/// Fill, hairline and the pressed and disabled states.
struct SyncProviderButtonStyle: ButtonStyle {
  let brand: SyncProviderButton.Brand
  let isDark: Bool
  let cornerRadius: CGFloat

  func makeBody(configuration: Configuration) -> some View {
    Face(configuration: configuration, brand: brand, isDark: isDark, cornerRadius: cornerRadius)
  }

  private struct Face: View {
    let configuration: Configuration
    let brand: SyncProviderButton.Brand
    let isDark: Bool
    let cornerRadius: CGFloat
    @Environment(\.isEnabled) private var isEnabled

    var body: some View {
      let shape = RoundedRectangle(cornerRadius: cornerRadius, style: .continuous)
      configuration.label
        .foregroundStyle(text)
        // Material's disabled state, which Google's buttons follow: 38%.
        .opacity(isEnabled ? 1 : 0.38)
        .background(fill, in: shape)
        .overlay {
          // Material's pressed state layer: the text colour at 12%.
          if configuration.isPressed { shape.fill(text.opacity(0.12)) }
        }
        .overlay(shape.strokeBorder(border.opacity(isEnabled ? 1 : 0.12), lineWidth: 1))
        .contentShape(shape)
    }

    // Google: #FFFFFF / #1F1F1F / #747775 light; #131314 / #E3E3E3 / #8E918F dark.
    // Apple: black with white in light mode, white with black in dark.
    private var fill: Color {
      switch brand {
      case .google: isDark ? Color(hex: 0x131314) : .white
      case .apple: isDark ? .white : .black
      }
    }

    private var text: Color {
      switch brand {
      case .google: Color(hex: isDark ? 0xE3E3E3 : 0x1F1F1F)
      case .apple: isDark ? .black : .white
      }
    }

    private var border: Color {
      switch brand {
      case .google: Color(hex: isDark ? 0x8E918F : 0x747775)
      case .apple: isDark ? .white : .black
      }
    }
  }
}

extension Color {
  fileprivate init(hex: UInt32) {
    self.init(
      red: Double((hex >> 16) & 0xFF) / 255, green: Double((hex >> 8) & 0xFF) / 255, blue: Double(hex & 0xFF) / 255)
  }
}
