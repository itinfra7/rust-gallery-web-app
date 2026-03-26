import { state } from './state.js';
import { createCardHtml } from './gallery-card.js';

function getColumnCount() {
    const width = window.innerWidth;
    if (width > 1800) return 6;
    if (width > 1500) return 5;
    if (width > 1200) return 4;
    if (width > 900) return 3;
    return 2;
}

export function renderGallery(images) {
    const container = document.getElementById('galleryContainer');
    container.innerHTML = '';
    if (images.length === 0) {
        document.getElementById('emptyState').style.display = 'block';
        return;
    } else {
        document.getElementById('emptyState').style.display = 'none';
    }

    const numCols = getColumnCount();
    const columns = [];
    for (let i = 0; i < numCols; i++) {
        const col = document.createElement('div');
        col.className = 'gallery-column';
        col.id = `col-${i}`;
        columns.push(col);
        container.appendChild(col);
    }

    images.forEach((img, i) => {
        const colIndex = i % numCols;
        const cardHtml = createCardHtml(img, i);
        columns[colIndex].insertAdjacentHTML('beforeend', cardHtml);
    });
}

export function appendGalleryDOM(newImages) {
    const container = document.getElementById('galleryContainer');
    const columns = Array.from(container.getElementsByClassName('gallery-column'));
    if (columns.length === 0) {
        renderGallery(newImages);
        return;
    }

    const numCols = columns.length;
    const startIndex = state.images.length - newImages.length;
    newImages.forEach((img, i) => {
        const globalIndex = startIndex + i;
        const colIndex = globalIndex % numCols;
        const cardHtml = createCardHtml(img, globalIndex);
        columns[colIndex].insertAdjacentHTML('beforeend', cardHtml);
    });
}

export function updateGalleryDOM(newImages) {
    renderGallery(newImages);
}

window.updateGalleryTranslations = function() {
    renderGallery(state.images);
};
