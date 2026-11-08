import Foundation
import Observation
import OSLog
import RevenueCat
import TaktSync

/// Takt Sync, the hosted server's £1-a-year subscription, bought here
/// through the App Store (docs/sync.md, "Subscription"). RevenueCat merges it
/// with Google Play's and tells the server, which is what lets every device
/// on the account sync; this only sells it and asks the server again after.
///
/// RevenueCat is logged in with the sync account's id, so a purchase belongs
/// to the account, not to this phone.
@MainActor
@Observable
final class TaktSyncPurchase {
  static let shared = TaktSyncPurchase()

  /// The App Store product: an auto-renewing yearly subscription.
  static let productID = "takt_sync_yearly"
  /// The RevenueCat entitlement it grants.
  static let entitlementID = "sync"

  /// The product as the App Store prices it here, once loaded.
  private(set) var product: StoreProduct?
  /// Whether RevenueCat says this account has Takt Sync.
  private(set) var isActive = false
  private(set) var isWorking = false
  /// The last thing that went wrong, for the settings screen.
  var problem: String?

  @ObservationIgnored private var account: String?
  @ObservationIgnored private let logger = Logger(subsystem: "uk.co.maybeitssoftware.takt", category: "TaktSyncPurchase")

  /// RevenueCat's public key from the build, or nil in a build without one.
  static var apiKey: String? {
    let key = (Bundle.main.object(forInfoDictionaryKey: "RevenueCatAPIKey") as? String)?
      .trimmingCharacters(in: .whitespacesAndNewlines)
    guard let key, !key.isEmpty, !key.hasPrefix("$(") else { return nil }
    return key
  }

  /// Whether purchases work in this build at all.
  var isAvailable: Bool { Self.apiKey != nil }

  /// Points RevenueCat at the signed-in sync account, configuring it on
  /// first use, and loads the price.
  func prepare(account accountID: String?) async {
    guard let key = Self.apiKey, let accountID, !accountID.isEmpty else { return }
    if !Purchases.isConfigured {
      Purchases.configure(withAPIKey: key, appUserID: accountID)
    } else if account != accountID {
      do {
        _ = try await Purchases.shared.logIn(accountID)
      } catch {
        logger.error("RevenueCat log-in failed: \(error.localizedDescription, privacy: .public)")
      }
    }
    account = accountID
    if product == nil {
      product = await Purchases.shared.products([Self.productID]).first
    }
    if let info = try? await Purchases.shared.customerInfo() {
      isActive = info.entitlements[Self.entitlementID]?.isActive == true
    }
  }

  /// Buys Takt Sync, then asks the server again so sync starts at once.
  func subscribe(then afterwards: @escaping () async -> Void) async {
    guard let product else {
      problem = "Takt Sync isn't available from the App Store right now."
      return
    }
    isWorking = true
    defer { isWorking = false }
    do {
      let result = try await Purchases.shared.purchase(product: product)
      if result.userCancelled { return }
      isActive = result.customerInfo.entitlements[Self.entitlementID]?.isActive == true
      problem = nil
      await afterwards()
    } catch {
      problem = error.localizedDescription
    }
  }

  /// Brings back a purchase made on this Apple ID before.
  func restore(then afterwards: @escaping () async -> Void) async {
    isWorking = true
    defer { isWorking = false }
    do {
      let info = try await Purchases.shared.restorePurchases()
      isActive = info.entitlements[Self.entitlementID]?.isActive == true
      problem = isActive ? nil : "No Takt Sync purchase was found on this Apple ID."
      await afterwards()
    } catch {
      problem = error.localizedDescription
    }
  }
}
