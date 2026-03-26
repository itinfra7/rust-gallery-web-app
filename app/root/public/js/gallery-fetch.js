import { state } from './state.js';
import { getFingerprint, unescapeHtml } from './utils.js';
import { translations } from './lang.js';
import { renderGallery, appendGalleryDOM } from './gallery-render.js';

function updateTotalCountUI() {
    const el = document.getElementById('totalCountDisplay');
    if (el) {
        el.setAttribute('data-count', state.totalCount);
        const dict = translations.en;
        if (dict) {
            el.innerText = dict.totalImages.replace('{n}', state.totalCount);
        }
    }
}

export async function fetchImages(append = false) {
    if (state.isLoading) return;
    state.isLoading = true;
    const fp = await getFingerprint();
    let url = `/api/images?sort=${state.currentSort}&page=${state.page}&limit=${state.limit}&fingerprint=${fp}`;

    if (state.currentTag) url += `&tag=${encodeURIComponent(state.currentTag)}`;
    if (state.currentSearch) url += `&search=${encodeURIComponent(state.currentSearch)}`;

    try {
        const res = await fetch(url);
        if (res.ok) {
            let newImages = await res.json();

            newImages = newImages.map(img => ({
                ...img,
                tags: img.tags ? img.tags.map(unescapeHtml).sort((a, b) => a.localeCompare(b)) : []
            }));

            const total = res.headers.get('X-Total-Count');

            if (total) {
                if (newImages.length > 0 || state.page === 1) {
                    state.totalCount = parseInt(total);
                    updateTotalCountUI();
                }
            }

            if (newImages.length < state.limit) state.hasMore = false;

            if (append) {
                state.images = [...state.images, ...newImages];
                appendGalleryDOM(newImages);
            } else {
                state.images = newImages;
                renderGallery(newImages);
            }
        }
    } catch (e) {
        console.error("Failed to fetch images");
    } finally {
        state.isLoading = false;
    }
}
