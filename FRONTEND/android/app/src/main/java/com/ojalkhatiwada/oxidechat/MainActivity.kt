package com.ojalkhatiwada.oxidechat

import android.annotation.SuppressLint
import android.content.pm.ApplicationInfo
import android.os.Bundle
import android.webkit.JavascriptInterface
import android.webkit.WebResourceRequest
import android.webkit.WebResourceResponse
import android.webkit.WebChromeClient
import android.webkit.WebSettings
import android.webkit.WebView
import androidx.appcompat.app.AppCompatActivity
import androidx.activity.result.contract.ActivityResultContracts
import androidx.webkit.WebViewAssetLoader
import androidx.webkit.WebViewClientCompat
import com.google.android.gms.auth.api.signin.GoogleSignIn
import com.google.android.gms.auth.api.signin.GoogleSignInOptions
import com.google.android.gms.common.api.ApiException
import com.google.firebase.auth.FirebaseAuth
import com.google.firebase.auth.GoogleAuthProvider
import org.json.JSONObject

class MainActivity : AppCompatActivity() {
    private val appAssetsHost = "localhost"
    private val appStartUrl = "https://$appAssetsHost/assets/index.html"
    private lateinit var webView: WebView
    private var firebaseAuth: FirebaseAuth? = null
    private val signInLauncher =
        registerForActivityResult(ActivityResultContracts.StartActivityForResult()) { result ->
            val task = GoogleSignIn.getSignedInAccountFromIntent(result.data)
            try {
                val account = task.getResult(ApiException::class.java)
                val idToken = account.idToken
                if (idToken.isNullOrBlank()) {
                    dispatchAuthError("Google sign-in did not return an ID token.")
                    return@registerForActivityResult
                }
                val credential = GoogleAuthProvider.getCredential(idToken, null)
                val auth = firebaseAuth
                if (auth == null) {
                    dispatchAuthError("Firebase is not configured for this Android build.")
                    return@registerForActivityResult
                }
                auth.signInWithCredential(credential).addOnCompleteListener { authResult ->
                    if (authResult.isSuccessful) {
                        dispatchCurrentAuthState()
                    } else {
                        val msg = authResult.exception?.localizedMessage
                            ?: "Firebase authentication failed."
                        dispatchAuthError(msg)
                    }
                }
            } catch (ex: ApiException) {
                dispatchAuthError("Google sign-in failed: ${ex.statusCode}")
            } catch (ex: Exception) {
                dispatchAuthError(ex.localizedMessage ?: "Google sign-in failed.")
            }
        }

    @SuppressLint("SetJavaScriptEnabled")
    override fun onCreate(savedInstanceState: Bundle?) {
        super.onCreate(savedInstanceState)
        firebaseAuth = runCatching { FirebaseAuth.getInstance() }.getOrNull()

        webView = WebView(this)
        setContentView(webView)

        val assetLoader = WebViewAssetLoader.Builder()
            .setDomain(appAssetsHost)
            .addPathHandler("/assets/", WebViewAssetLoader.AssetsPathHandler(this))
            .build()

        webView.webViewClient = object : WebViewClientCompat() {
            override fun shouldInterceptRequest(
                view: WebView,
                request: WebResourceRequest
            ): WebResourceResponse? {
                return assetLoader.shouldInterceptRequest(request.url)
            }
        }
        webView.webChromeClient = WebChromeClient()
        webView.addJavascriptInterface(AndroidAuthBridge(), "AndroidAuth")

        webView.settings.apply {
            javaScriptEnabled = true
            domStorageEnabled = true
            cacheMode = WebSettings.LOAD_DEFAULT
            val isDebuggable = (applicationInfo.flags and ApplicationInfo.FLAG_DEBUGGABLE) != 0
            mixedContentMode = if (isDebuggable) {
                WebSettings.MIXED_CONTENT_COMPATIBILITY_MODE
            } else {
                WebSettings.MIXED_CONTENT_NEVER_ALLOW
            }
            allowFileAccess = false
            allowContentAccess = false
            javaScriptCanOpenWindowsAutomatically = true
            setSupportMultipleWindows(true)
        }

        webView.loadUrl(appStartUrl)
    }

    override fun onBackPressed() {
        if (webView.canGoBack()) {
            webView.goBack()
        } else {
            super.onBackPressed()
        }
    }

    private inner class AndroidAuthBridge {
        @JavascriptInterface
        fun signInWithGoogle() {
            runOnUiThread {
                val webClientId = resolveDefaultWebClientId()
                if (webClientId.isBlank()) {
                    dispatchAuthError("Missing default_web_client_id. Add Firebase Android config to enable native sign-in.")
                    return@runOnUiThread
                }
                val gso = GoogleSignInOptions.Builder(GoogleSignInOptions.DEFAULT_SIGN_IN)
                    .requestIdToken(webClientId)
                    .requestEmail()
                    .build()
                val client = GoogleSignIn.getClient(this@MainActivity, gso)
                signInLauncher.launch(client.signInIntent)
            }
        }

        @JavascriptInterface
        fun signOut() {
            runOnUiThread {
                firebaseAuth?.signOut()
                dispatchSignedOut()
            }
        }

        @JavascriptInterface
        fun getCurrentUserJson(): String {
            val user = firebaseAuth?.currentUser ?: return "null"
            val payload = JSONObject()
            payload.put("uid", user.uid)
            payload.put("displayName", user.displayName ?: "")
            payload.put("email", user.email ?: "")
            payload.put("photoURL", user.photoUrl?.toString() ?: "")
            return payload.toString()
        }
    }

    private fun dispatchCurrentAuthState() {
        val user = firebaseAuth?.currentUser
        if (user == null) {
            dispatchSignedOut()
            return
        }

        val payload = JSONObject()
        payload.put("uid", user.uid)
        payload.put("displayName", user.displayName ?: "")
        payload.put("email", user.email ?: "")
        payload.put("photoURL", user.photoUrl?.toString() ?: "")
        val escaped = JSONObject.quote(payload.toString())
        webView.evaluateJavascript("window.onAndroidAuthResult && window.onAndroidAuthResult($escaped);", null)
    }

    private fun dispatchSignedOut() {
        webView.evaluateJavascript("window.onAndroidSignedOut && window.onAndroidSignedOut();", null)
    }

    private fun dispatchAuthError(message: String) {
        val escaped = JSONObject.quote(message)
        webView.evaluateJavascript("window.onAndroidAuthError && window.onAndroidAuthError($escaped);", null)
    }

    private fun resolveDefaultWebClientId(): String {
        val resId = resources.getIdentifier("default_web_client_id", "string", packageName)
        if (resId == 0) {
            return ""
        }
        return runCatching { getString(resId) }.getOrDefault("")
    }
}
