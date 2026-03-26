export function escapeHtml(text) {
    if (!text) return text;
    return text
        .replace(/&/g, "&amp;")
        .replace(/</g, "&lt;")
        .replace(/>/g, "&gt;")
        .replace(/"/g, "&quot;")
        .replace(/'/g, "&#039;");
}

export function unescapeHtml(text) {
    if (!text) return text;
    return text
        .replace(/&amp;/g, "&")
        .replace(/&lt;/g, "<")
        .replace(/&gt;/g, ">")
        .replace(/&quot;/g, "\"")
        .replace(/&#039;/g, "'")
        .replace(/&#x27;/g, "'");
}

export function escapeJsStr(text) {
    if (!text) return text;
    return text
        .replace(/\\/g, '\\\\')
        .replace(/'/g, "\\'");
}

export async function getFingerprint() {
    let stored = localStorage.getItem('gallery_fp');
    if (stored) return stored;

    const str = navigator.userAgent + navigator.language + screen.colorDepth + screen.width + new Date().getTimezoneOffset();
    let hash = 0;
    for (let i = 0; i < str.length; i++) {
        const char = str.charCodeAt(i);
        hash = ((hash << 5) - hash) + char;
        hash = hash & hash;
    }
    const fp = Math.abs(hash).toString(16);
    localStorage.setItem('gallery_fp', fp);
    return fp;
}
