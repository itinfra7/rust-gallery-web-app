import { state } from './state.js';
import { initTheme, toggleTheme } from './theme.js';
import { initGallery } from './gallery-init.js';
import { updateGalleryDOM } from './gallery-render.js';
import { applySort, filterByTag, clearFilter, handleSearch, clearSearchInput } from './gallery-filters.js';
import { toggleLike, syncLikeStates } from './gallery-actions.js';
import { initModal, openImageModal, closeImageModal, changeImage, openLegalModal, closeLegalModal } from './modal.js';
import { openLoginModal, closeLoginModal, submitLogin, logout } from './auth.js';
import { initUpload, openUploadModal, closeUploadModal, submitUpload } from './upload.js';
import { deleteSelectedImages, handleCheck } from './admin-check.js';
import { editTags } from './admin-tags.js';
import { fetchSuggestions } from './autocomplete-data.js';
import { setupAutocomplete } from './autocomplete-core.js';
import { initLanguage } from './lang.js';
import { initComments } from './comments-view.js';
import { unescapeHtml } from './utils.js';
import { initContact } from './contact.js';
import { initSearchHelp } from './search-help.js';
import './comments-actions.js';

document.addEventListener('DOMContentLoaded', async () => {
    initTheme();
    initLanguage();
    initModal();
    initUpload();
    initGallery();
    initComments();
    initContact();
    initSearchHelp();
    await fetchSuggestions();

    if (typeof initialImages !== 'undefined') {
        state.images = initialImages.map(img => ({
            ...img,
            tags: img.tags ? img.tags.map(unescapeHtml).sort((a, b) => a.localeCompare(b)) : []
        }));

        if (state.images.length < state.limit) {
            state.hasMore = false;
        }
        updateGalleryDOM(state.images);
        await syncLikeStates();
    }

    if (typeof initialTotalCount !== 'undefined') {
        state.totalCount = initialTotalCount;
        const el = document.getElementById('totalCountDisplay');
        if (el) {
            el.setAttribute('data-count', state.totalCount);
            const langModule = await import('./lang.js');
            const dict = langModule.translations.en;
            el.innerText = dict.totalImages.replace('{n}', state.totalCount);
        }
    }

    const searchInput = document.getElementById('searchInput');
    if (searchInput) {
        setupAutocomplete(searchInput, state.searchSuggestions, false);
        searchInput.addEventListener('input', (e) => {
            handleSearch(e.target.value);
        });
    }
    
    const searchClearBtn = document.getElementById('searchClearBtn');
    if (searchClearBtn) {
        searchClearBtn.addEventListener('click', clearSearchInput);
    }

    const loginModal = document.getElementById('loginModal');
    const imgModal = document.getElementById('imageModal');

    document.addEventListener('keydown', (e) => {
        if (imgModal.style.display === "flex") {
            if (e.key === "Escape") closeImageModal();
            else if (e.key === "ArrowLeft") changeImage(-1);
            else if (e.key === "ArrowRight") changeImage(1);
        }

        if (loginModal.style.display === "flex") {
            if (e.key === "Escape") closeLoginModal();
            if (e.key === "Enter") submitLogin();
        }
    });
});

window.toggleTheme = toggleTheme;
window.applySort = applySort;
window.filterByTag = filterByTag;
window.clearFilter = clearFilter;
window.openImageModal = openImageModal;
window.closeImageModal = closeImageModal;
window.changeImage = changeImage;
window.openLoginModal = openLoginModal;
window.closeLoginModal = closeLoginModal;
window.submitLogin = submitLogin;
window.logout = logout;
window.openUploadModal = openUploadModal;
window.closeUploadModal = closeUploadModal;
window.submitUpload = submitUpload;
window.deleteSelectedImages = deleteSelectedImages;
window.handleCheck = handleCheck;
window.toggleLike = toggleLike;
window.editTags = editTags;
window.openLegalModal = openLegalModal;
window.closeLegalModal = closeLegalModal;
