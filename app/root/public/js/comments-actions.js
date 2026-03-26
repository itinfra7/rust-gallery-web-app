import { state } from './state.js';
import { getFingerprint } from './utils.js';
import { translations } from './lang.js';
import { showToast } from './toast.js';
import { loadComments } from './comments-view.js';

let fingerprint = null;

function getDict() {
    return translations.en;
}

function updateGalleryCardCommentCount(id, count) {
    const cards = document.querySelectorAll('.card');
    cards.forEach(card => {
        const heart = card.querySelector(`#heart-${id}`);
        if (heart) {
            const indicator = heart.parentElement.parentElement.querySelector('.comment-count');
            if (indicator) indicator.innerText = count;
        }
    });
}

export async function submitComment() {
    const input = document.getElementById('commentInput');
    const content = input.value.trim();
    const dict = getDict();

    if (!content) return;
    if (!fingerprint) fingerprint = await getFingerprint();

    const imageId = state.images[state.currentIndex].id;

    try {
        const res = await fetch(`/api/image/${imageId}/comments`, {
            method: 'POST',
            headers: { 'Content-Type': 'application/json' },
            body: JSON.stringify({ content: content, fingerprint: fingerprint })
        });

        const msgCode = await res.text();
        const msg = dict[msgCode] || msgCode;

        if (res.ok) {
            input.value = '';
            document.getElementById('charCount').innerText = '0/100';
            loadComments(imageId);

            state.images[state.currentIndex].comment_count++;
            const currentImg = state.images[state.currentIndex];
            if (state.images.indexOf(currentImg) !== -1) {
               updateGalleryCardCommentCount(currentImg.id, currentImg.comment_count);
            }
            showToast(msg, 'success');
        } else {
            showToast(msg, 'error');
        }
    } catch (e) {
        showToast('Network error', 'error');
    }
}

export async function deleteComment(commentId) {
    const dict = getDict();
    if (!confirm(dict.commentDeleteConfirm)) return;

    try {
        const res = await fetch(`/api/comment/${commentId}`, { method: 'DELETE' });
        const msgCode = await res.text();
        const msg = dict[msgCode] || msgCode;

        if (res.ok) {
            const imageId = state.images[state.currentIndex].id;
            state.images[state.currentIndex].comment_count--;
            updateGalleryCardCommentCount(imageId, state.images[state.currentIndex].comment_count);
            loadComments(imageId);
            showToast(msg, 'success');
        } else {
            showToast(msg, 'error');
        }
    } catch (e) {
        showToast('Network error', 'error');
    }
}

window.deleteComment = deleteComment;
window.submitComment = submitComment;
