package com.stellasecret.cvgenerator.data.repository

import android.content.Context
import android.net.Uri
import com.tom_roush.pdfbox.pdmodel.PDDocument
import com.tom_roush.pdfbox.text.PDFTextStripper
import dagger.hilt.android.qualifiers.ApplicationContext
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.withContext
import javax.inject.Inject
import javax.inject.Singleton

@Singleton
class DocumentRepository @Inject constructor(
    @ApplicationContext private val context: Context
) {
    /**
     * Extracts text from a document URI.
     * Supports: PDF, TXT, DOC, DOCX (basic), and other text-based formats.
     */
    suspend fun extractText(uri: Uri): Result<String> = withContext(Dispatchers.IO) {
        try {
            val mimeType = context.contentResolver.getType(uri) ?: ""
            val fileName = getFileName(uri)

            val text = when {
                mimeType.contains("pdf") || fileName.endsWith(".pdf", ignoreCase = true) ->
                    extractFromPdf(uri)

                mimeType.contains("text") || fileName.endsWith(".txt", ignoreCase = true) ->
                    extractFromText(uri)

                mimeType.contains("wordprocessingml") ||
                        fileName.endsWith(".docx", ignoreCase = true) ->
                    extractFromDocx(uri)

                fileName.endsWith(".doc", ignoreCase = true) ->
                    extractFromText(uri) // Fallback for old .doc

                else ->
                    extractFromText(uri) // Generic fallback
            }

            if (text.isBlank()) {
                Result.failure(Exception("Le document semble vide ou illisible"))
            } else {
                Result.success(text)
            }
        } catch (e: Exception) {
            Result.failure(Exception("Impossible de lire le fichier : ${e.message}"))
        }
    }

    private fun extractFromPdf(uri: Uri): String {
        val inputStream = context.contentResolver.openInputStream(uri)
            ?: throw Exception("Cannot open PDF stream")
        return inputStream.use { stream ->
            val document = PDDocument.load(stream)
            document.use {
                val stripper = PDFTextStripper()
                stripper.sortByPosition = true
                stripper.getText(it)
            }
        }
    }

    private fun extractFromText(uri: Uri): String {
        val inputStream = context.contentResolver.openInputStream(uri)
            ?: throw Exception("Cannot open file stream")
        return inputStream.bufferedReader(Charsets.UTF_8).use { it.readText() }
    }

    private fun extractFromDocx(uri: Uri): String {
        // Basic DOCX text extraction using Apache POI
        return try {
            val inputStream = context.contentResolver.openInputStream(uri)
                ?: throw Exception("Cannot open DOCX stream")
            inputStream.use { stream ->
                // Use POI to extract text
                val document = org.apache.poi.xwpf.usermodel.XWPFDocument(stream)
                val extractor = org.apache.poi.xwpf.extractor.XWPFWordExtractor(document)
                extractor.use { it.text }
            }
        } catch (e: Exception) {
            // Fallback: try reading as text
            extractFromText(uri)
        }
    }

    private fun getFileName(uri: Uri): String {
        var fileName = ""
        context.contentResolver.query(uri, null, null, null, null)?.use { cursor ->
            if (cursor.moveToFirst()) {
                val nameIndex = cursor.getColumnIndex(android.provider.OpenableColumns.DISPLAY_NAME)
                if (nameIndex >= 0) {
                    fileName = cursor.getString(nameIndex) ?: ""
                }
            }
        }
        return fileName
    }
}
