# Keep GoogleDriveHelper — startSignIn() called only via JNI, not bytecode
-keep class com.stellasecret.cvgenerator.GoogleDriveHelper { *; }

# Keep PdfExporter — exportPdf() called only via JNI, not bytecode
-keep class com.stellasecret.cvgenerator.PdfExporter { *; }
