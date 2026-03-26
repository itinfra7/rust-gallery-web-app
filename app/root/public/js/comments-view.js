import { escapeHtml } from './utils.js';
import { translations } from './lang.js';

function getDict() {
    return translations.en;
}

export async function loadComments(imageId) {
    const list = document.getElementById('commentList');
    const header = document.querySelector('.comments-header');
    const dict = getDict();

    list.innerHTML = `<div style="text-align:center; padding:10px; color:#666;">${dict.commentLoading}</div>`;

    try {
        const res = await fetch(`/api/image/${imageId}/comments`);
        if (res.ok) {
            const comments = await res.json();
            if (header) {
                header.dataset.count = comments.length;
                header.innerText = dict.commentsHeaderCount.replace('{n}', comments.length);
            }
            renderComments(comments);
        } else {
            list.innerHTML = '';
        }
    } catch (e) {
        list.innerHTML = `<div style="text-align:center; padding:10px; color:#d32f2f;">${dict.commentError}</div>`;
    }
}

function renderComments(comments) {
    const list = document.getElementById('commentList');
    const dict = getDict();
    list.innerHTML = '';

    if (comments.length === 0) {
        list.innerHTML = `<div style="text-align:center; padding:10px; color:#666; font-size:12px;">${dict.commentNoComments}</div>`;
        return;
    }

    comments.forEach(c => {
        const item = document.createElement('div');
        item.className = 'comment-item';
        item.innerHTML = `
            <button class="comment-delete-btn" onclick="deleteComment('${c.id}')">&times;</button>
            <div class="comment-text">${escapeHtml(c.content)}</div>
            <div class="comment-meta">
                <span>${c.created_at}</span>
                <span>${c.ip_masked}</span>
                <span>${c.browser}</span>
                <span>#${c.fingerprint_short}</span>
            </div>
        `;
        list.appendChild(item);
    });
}

export async function initComments() {
    const input = document.getElementById('commentInput');
    const count = document.getElementById('charCount');

    if (input) {
        input.addEventListener('input', () => {
            const len = input.value.length;
            count.innerText = `${len}/100`;
            if (len > 100) {
                count.style.color = '#d32f2f';
                input.value = input.value.substring(0, 100);
            } else {
                count.style.color = '#666';
            }
        });
    }
}
