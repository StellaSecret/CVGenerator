package com.stellasecret.cvgenerator

import android.app.Activity
import android.content.Context
import android.print.PrintAttributes
import android.print.PrintManager
import android.util.Log
import android.view.ViewGroup
import android.webkit.WebView
import android.webkit.WebViewClient
import android.widget.FrameLayout

object PdfExporter {
    private const val TAG = "PdfExporter"
    private var activity: Activity? = null
    private var cachedWebView: WebView? = null

    fun init(act: Activity) {
        activity = act
        Log.d(TAG, "init")
    }

    // Called from Rust via JNI (services/android_pdf.rs). Android's WebView
    // has no window.print(), so unlike the web build we can't drive the
    // browser's print-to-PDF from inside the app's own WebView. Instead,
    // load the already-rendered CV HTML (srcdoc of the preview iframe) into
    // a dedicated off-screen WebView and open the system print dialog —
    // whose "Save as PDF" destination produces the PDF file.
    @JvmStatic
    fun exportPdf(html: String, filename: String) {
        val act = activity ?: run {
            Log.e(TAG, "exportPdf: activity is null (init not called?)")
            return
        }
        val jobName = filename.removeSuffix(".pdf").ifBlank { "CV Generator" }
        act.runOnUiThread {
            val webView = cachedWebView ?: run {
                // Kept attached (invisible 1x1) for the app's lifetime so the
                // print adapter always has a laid-out surface to render — an
                // unattached/unmeasured WebView can produce a blank PDF on
                // some devices. Cached rather than recreated to avoid piling
                // up an extra WebView+host per download click.
                val wv = WebView(act)
                wv.settings.javaScriptEnabled = false
                wv.isVerticalScrollBarEnabled = false
                wv.isHorizontalScrollBarEnabled = false
                val host = FrameLayout(act)
                host.addView(wv, ViewGroup.LayoutParams(1, 1))
                act.findViewById<ViewGroup>(android.R.id.content)?.addView(host)
                cachedWebView = wv
                wv
            }
            webView.webViewClient = object : WebViewClient() {
                override fun onPageFinished(view: WebView?, url: String?) {
                    // Must come from the Activity itself: PrintManager.print()
                    // throws IllegalStateException if its context isn't one.
                    val pm = act.getSystemService(Context.PRINT_SERVICE) as? PrintManager
                    if (pm == null) {
                        Log.e(TAG, "exportPdf: no PrintManager service")
                        return
                    }
                    val doc = webView.createPrintDocumentAdapter(jobName)
                    val attrs =
                        PrintAttributes.Builder()
                            .setMediaSize(PrintAttributes.MediaSize.ISO_A4)
                            .setMinMargins(PrintAttributes.Margins.NO_MARGINS)
                            .build()
                    pm.print(jobName, doc, attrs)
                    Log.d(TAG, "exportPdf: print job submitted ($jobName)")
                }
            }
            webView.loadDataWithBaseURL(null, html, "text/html", "utf-8", null)
        }
    }
}