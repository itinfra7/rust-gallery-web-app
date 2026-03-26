import { state } from './state.js';
import { fetchImages } from './gallery-fetch.js';
import { closeImageModal } from './modal.js';

let searchDebounceTimer;

export async function applySort(order) {
    state.currentSort = order;
    state.page = 1;
    state.hasMore = true;
    state.images = [];
    state.lastCheckedIndex = null;

    document.querySelectorAll('.menu-item').forEach(el => el.classList.remove('active'));
    document.getElementById(`sort${order.charAt(0).toUpperCase() + order.slice(1)}`).classList.add('active');

    fetchImages(false);
}

export async function filterByTag(tag, event) {
    if (event) event.stopPropagation();
    state.currentTag = tag;
    state.page = 1;
    state.hasMore = true;
    state.images = [];
    state.lastCheckedIndex = null;

    const input = document.getElementById('searchInput');
    if (input) {
        input.value = tag;
    }
    
    setTimeout(() => updateSearchClearBtn(true), 10);
    
    fetchImages(false);
    if (document.getElementById('imageModal').style.display === "flex") {
        closeImageModal();
    }
}

export function clearFilter() {
    state.currentTag = null;
    state.currentSearch = '';
    state.page = 1;
    state.hasMore = true;
    state.images = [];
    state.lastCheckedIndex = null;

    const searchInput = document.getElementById('searchInput');
    if (searchInput) {
        searchInput.value = '';
        searchInput.focus();
    }
    updateSearchClearBtn(false);

    fetchImages(false);
}

export function handleSearch(query) {
    updateSearchClearBtn(query.length > 0);
    clearTimeout(searchDebounceTimer);
    searchDebounceTimer = setTimeout(() => {
        state.currentSearch = query.trim();
        state.page = 1;
        state.hasMore = true;
        state.images = [];
        fetchImages(false);
    }, 300);
}

export function updateSearchClearBtn(show) {
    const btn = document.getElementById('searchClearBtn');
    if (btn) {
        if (show) {
            btn.classList.add('show');
            btn.style.display = 'flex';
        } else {
            btn.classList.remove('show');
            btn.style.display = 'none';
        }
    }
}

export function clearSearchInput() {
    clearFilter();
}
