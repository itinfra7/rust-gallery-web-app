export const translations = {
    en: {
        sortDesc: "Newest",
        sortAsc: "Oldest",
        sortPopular: "Popular",
        sortComments: "Comments",
        legalBtn: "Legal",
        deleteSelected: "Delete Selected",
        uploadImage: "Upload Image",
        dashboard: "Dashboard",
        logout: "Logout",
        login: "Login",
        searchPlaceholder: "Search tags...",
        totalImages: "Total {n} images",
        noImages: "No images found.",
        relatedImages: "Related Images",
        loginTitle: "Admin Access",
        authenticate: "Authenticate",
        uploadTitle: "Upload Image",
        dropText: "Drag & Drop files here or Click",
        uploadBtn: "Upload",
        filesSelected: "{n} files selected",
        uploading: "Uploading file {n} of {t}: {f}",
        complete: "All transfers complete. Reloading...",
        error: "Error uploading {f}",
        networkError: "Network Error on {f}",
        legalTitle: "Legal & Copyright Notice",
        commentsHeader: "Comments",
        commentsHeaderCount: "Comments ({n})",
        commentPlaceholder: "Write a comment (max 100 chars)...",
        commentPostBtn: "Post",
        commentNoComments: "No comments yet.",
        commentLoading: "Loading...",
        commentError: "Error loading comments",
        commentDeleteConfirm: "Delete this comment?",
        commentDeleteFailed: "Delete failed",
        commentPostFailed: "Failed to post comment",
        noTags: "No Tags",
        accessDenied: "Access Denied",
        error_daily_limit: "Daily limit reached for this image.",
        error_comment_image_limit: "You can post at most 3 comments on one image.",
        error_content_length: "Invalid content length.",
        error_unauthorized: "Unauthorized.",
        error_login_rate_limited: "Too many login attempts. Try again later.",
        error_comment_rate_limited: "You are commenting too quickly. Try again in a minute.",
        error_like_rate_limited: "You are toggling likes too quickly. Try again in a minute.",
        success_comment_added: "Comment added.",
        success_comment_deleted: "Comment deleted.",
        liked: "Liked",
        unliked: "Unliked",
        likeError: "Action failed",
        clickToReveal: "Click to reveal email"
    }
};

export function initLanguage() {
    setLanguage('en');
}

export function setLanguage() {
    const dict = translations.en;

    document.querySelectorAll('[data-i18n]').forEach(el => {
        const key = el.getAttribute('data-i18n');
        if (dict[key]) {
            el.innerText = dict[key];
        }
    });

    const searchInput = document.getElementById('searchInput');
    if (searchInput) {
        searchInput.placeholder = dict.searchPlaceholder;
    }

    const commentInput = document.getElementById('commentInput');
    if (commentInput) {
        commentInput.placeholder = dict.commentPlaceholder;
    }

    const totalCountEl = document.getElementById('totalCountDisplay');
    if (totalCountEl) {
        const count = totalCountEl.getAttribute('data-count');
        totalCountEl.innerText = dict.totalImages.replace('{n}', count);
    }

    const commentsHeader = document.querySelector('.comments-header');
    if (commentsHeader && commentsHeader.dataset.count !== undefined) {
        const count = commentsHeader.dataset.count;
        commentsHeader.innerText = dict.commentsHeaderCount.replace('{n}', count);
    }

    if (typeof window.updateGalleryTranslations === 'function') {
        window.updateGalleryTranslations();
    }
}

window.setLanguage = setLanguage;
