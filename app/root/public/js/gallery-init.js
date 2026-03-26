import { state } from './state.js';
import { fetchImages } from './gallery-fetch.js';
import { renderGallery } from './gallery-render.js';

let resizeDebounceTimer;

export function initGallery() {
    initInfiniteScroll();
    window.addEventListener('resize', () => {
        clearTimeout(resizeDebounceTimer);
        resizeDebounceTimer = setTimeout(() => {
            renderGallery(state.images);
        }, 200);
    });
}

export function initInfiniteScroll() {
    window.addEventListener('scroll', () => {
        if (state.isLoading || !state.hasMore) return;
        if (window.innerHeight + window.scrollY >= document.body.offsetHeight - 500) {
            state.page++;
            fetchImages(true);
        }
    });
}
