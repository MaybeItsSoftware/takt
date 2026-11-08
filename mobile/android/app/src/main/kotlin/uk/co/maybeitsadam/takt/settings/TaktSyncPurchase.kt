package uk.co.maybeitsadam.takt.settings

import android.app.Activity
import android.content.Context
import com.revenuecat.purchases.CustomerInfo
import com.revenuecat.purchases.PurchaseParams
import com.revenuecat.purchases.Purchases
import com.revenuecat.purchases.PurchasesConfiguration
import com.revenuecat.purchases.PurchasesTransactionException
import com.revenuecat.purchases.awaitCustomerInfo
import com.revenuecat.purchases.awaitGetProducts
import com.revenuecat.purchases.awaitLogIn
import com.revenuecat.purchases.awaitPurchase
import com.revenuecat.purchases.awaitRestore
import com.revenuecat.purchases.models.StoreProduct
import kotlinx.coroutines.CancellationException
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.StateFlow
import kotlinx.coroutines.flow.asStateFlow
import uk.co.maybeitsadam.takt.BuildConfig

/**
 * Takt Sync, the hosted server's £1-a-year subscription, bought here through
 * Google Play (docs/sync.md, "Subscription"). RevenueCat merges it with the
 * App Store's and tells the server, which is what lets every device on the
 * account sync; this only sells it and asks the server again afterwards.
 * RevenueCat is logged in with the sync account's id, so the purchase belongs
 * to the account, not to this phone.
 */
class TaktSyncPurchase(private val context: Context) {
    /** What the settings screen draws. */
    data class State(
        val price: String? = null,
        val active: Boolean = false,
        val working: Boolean = false,
        val problem: String? = null,
    )

    private val _state = MutableStateFlow(State())
    val state: StateFlow<State> = _state.asStateFlow()

    private var product: StoreProduct? = null
    private var account: String? = null

    /** Whether purchases work in this build at all. */
    val isAvailable: Boolean get() = BuildConfig.REVENUECAT_KEY.isNotBlank()

    /** Points RevenueCat at the signed-in sync account, configuring it once, and loads the price. */
    suspend fun prepare(accountId: String?) {
        if (!isAvailable || accountId.isNullOrBlank()) return
        try {
            if (!Purchases.isConfigured) {
                Purchases.configure(PurchasesConfiguration.Builder(context.applicationContext, BuildConfig.REVENUECAT_KEY).appUserID(accountId).build())
            } else if (account != accountId) {
                Purchases.sharedInstance.awaitLogIn(accountId)
            }
            account = accountId
            if (product == null) product = Purchases.sharedInstance.awaitGetProducts(listOf(PRODUCT_ID)).firstOrNull()
            _state.value = _state.value.copy(price = product?.price?.formatted, active = Purchases.sharedInstance.awaitCustomerInfo().hasSync())
        } catch (cancelled: CancellationException) {
            throw cancelled
        } catch (error: Exception) {
            _state.value = _state.value.copy(problem = error.message)
        }
    }

    /** Buys Takt Sync, then [afterwards] asks the server again so sync starts at once. */
    suspend fun subscribe(activity: Activity, afterwards: suspend () -> Unit) {
        val chosen = product ?: run {
            _state.value = _state.value.copy(problem = "Takt Sync isn't available from Google Play right now.")
            return
        }
        _state.value = _state.value.copy(working = true, problem = null)
        try {
            val result = Purchases.sharedInstance.awaitPurchase(PurchaseParams.Builder(activity, chosen).build())
            _state.value = _state.value.copy(active = result.customerInfo.hasSync())
            afterwards()
        } catch (cancelled: CancellationException) {
            throw cancelled
        } catch (refused: PurchasesTransactionException) {
            if (!refused.userCancelled) _state.value = _state.value.copy(problem = refused.message)
        } catch (error: Exception) {
            _state.value = _state.value.copy(problem = error.message)
        } finally {
            _state.value = _state.value.copy(working = false)
        }
    }

    /** Brings back a purchase made on this Google account before. */
    suspend fun restore(afterwards: suspend () -> Unit) {
        _state.value = _state.value.copy(working = true, problem = null)
        try {
            val active = Purchases.sharedInstance.awaitRestore().hasSync()
            _state.value = _state.value.copy(
                active = active,
                problem = if (active) null else "No Takt Sync purchase was found on this Google account.",
            )
            afterwards()
        } catch (cancelled: CancellationException) {
            throw cancelled
        } catch (error: Exception) {
            _state.value = _state.value.copy(problem = error.message)
        } finally {
            _state.value = _state.value.copy(working = false)
        }
    }

    private fun CustomerInfo.hasSync(): Boolean = entitlements[ENTITLEMENT_ID]?.isActive == true

    companion object {
        /** The Play product: an auto-renewing yearly subscription. */
        const val PRODUCT_ID = "takt_sync_yearly"

        /** The RevenueCat entitlement it grants. */
        const val ENTITLEMENT_ID = "sync"
    }
}
