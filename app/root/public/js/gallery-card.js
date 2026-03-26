import { escapeHtml, escapeJsStr } from './utils.js';
import { translations } from './lang.js';

function buildFallbackAlt(img) {
    if (img.tags && img.tags.length > 0) {
        return `Image tagged ${img.tags.slice(0, 8).join(', ')}`;
    }
    if (img.date) {
        return `Curated image on <PUBLIC_DOMAIN> uploaded on ${img.date}`;
    }
    return 'Curated image on <PUBLIC_DOMAIN>';
}

export function createCardHtml(img, index) {
    const isAdmin = document.body.dataset.admin === 'true';
    const dict = translations.en;

    let playIconHtml = '';
    if (img.is_animated) {
        playIconHtml = `
            <div class="play-icon">
                <svg viewBox="0 0 24 24"><path d="M8 5v14l11-7z"/></svg>
            </div>`;
    }

    let tagsHtml = '';
    if (img.tags && img.tags.length > 0) {
        tagsHtml = img.tags.map(tag =>
            `<span class="tag-item" onclick="filterByTag('${escapeJsStr(tag)}', event)">${escapeHtml(tag)}</span>`
        ).join('');
    } else {
        tagsHtml = dict.noTags;
    }

    const likedClass = img.is_liked ? 'liked' : '';
    const commentCount = img.comment_count || 0;
    const pageUrl = img.page_url || `/image/${img.id}`;
    const altText = img.alt_text || buildFallbackAlt(img);
    const thumbUrl = img.thumb_url || `/uploads/thumbs/${img.thumb}`;

    const metaRightHtml = `
        <div class="meta-right">
            <div class="comment-indicator">
                <span class="comment-count">${commentCount}</span>
                <svg class="comment-icon" viewBox="0 0 24 24"><path d="M20 2H4c-1.1 0-2 .9-2 2v18l4-4h14c1.1 0 2-.9 2-2V4c0-1.1-.9-2-2-2z"/></svg>
            </div>
            <div class="heart-container ${likedClass}" id="heart-${img.id}" onclick="toggleLike('${img.id}', event)">
                <span class="heart-count" id="count-${img.id}">${img.like_count}</span>
                <svg class="heart-icon" viewBox="0 0 24 24"><path d="M12 21.35l-1.45-1.32C5.4 15.36 2 12.28 2 8.5 2 5.42 4.42 3 7.5 3c1.74 0 3.41.81 4.5 2.09C13.09 3.81 14.76 3 16.5 3 19.58 3 22 5.42 22 8.5c0 3.78-3.4 6.86-8.55 11.54L12 21.35z"/></svg>
            </div>
        </div>`;

    let adminCheckHtml = '', infoHtml = '';

    if (isAdmin) {
        adminCheckHtml = `
            <label class="check-overlay" onclick="event.stopPropagation()">
                <input type="checkbox" class="delete-check" id="delete-check-${index}" value="${escapeHtml(img.name)}" onclick="handleCheck(event, ${index})">
                <span class="checkmark"></span>
            </label>`;

        infoHtml = `
            <div class="card-info" onclick="event.stopPropagation()">
                <div class="meta-row"><div class="card-date">${img.date}</div>${metaRightHtml}</div>
                <div class="info-row">
                    <div class="info-text tags-text" id="tags-text-${img.id}">${tagsHtml}</div>
                    <button class="edit-btn" onclick="editTags('${img.id}')"><svg viewBox="0 0 24 24"><path d="M3 17.25V21h3.75L17.81 9.94l-3.75-3.75L3 17.25zM20.71 7.04c.39-.39.39-1.02 0-1.41l-2.34-2.34c-.39-.39-1.02-.39-1.41 0l-1.83 1.83 3.75 3.75 1.83-1.83z"/></svg></button>
                </div>
            </div>`;
    } else {
        infoHtml = `
            <div class="card-info" onclick="event.stopPropagation()">
                <div class="meta-row"><div class="card-date">${img.date}</div>${metaRightHtml}</div>
                <div class="info-row"><div class="info-text tags-text">${tagsHtml}</div></div>
            </div>`;
    }

    return `
        <div class="card">
            <div class="card-inner" onclick="openImageModal(${index})">
                <a class="card-link" href="${pageUrl}" onclick="if (window.openImageModal) { event.preventDefault(); event.stopPropagation(); openImageModal(${index}); }">
                    ${playIconHtml}
                    <img class="card-img" src="${thumbUrl}" alt="${escapeHtml(altText)}" loading="lazy">
                </a>
                ${adminCheckHtml}
            </div>
            ${infoHtml}
        </div>`;
}
