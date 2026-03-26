import { state } from './state.js';
import { getFingerprint } from './utils.js';
import { translations } from './lang.js';
import { showToast } from './toast.js';
import { updateModalLikeState } from './modal.js';

export async function syncLikeStates() {
    if (state.images.length === 0) return;
    const fp = await getFingerprint();
    const ids = state.images.map(img => img.id);
    try {
        const res = await fetch('/api/likes/check', {
            method: 'POST',
            headers: { 'Content-Type': 'application/json' },
            body: JSON.stringify({ fingerprint: fp, image_ids: ids })
        });
        if (res.ok) {
            const likedIds = await res.json();
            state.images.forEach(img => {
                if (likedIds.includes(img.id)) {
                    img.is_liked = true;
                    const heart = document.getElementById(`heart-${img.id}`);
                    if (heart) heart.classList.add('liked');
                }
            });
        }
    } catch (e) {
        console.error("Failed to sync likes");
    }
}

export async function toggleLike(id, event) {
    if (event) event.stopPropagation();
    if (state.isLikeProcessing) return;
    state.isLikeProcessing = true;
    const fp = await getFingerprint();
    const dict = translations.en;
    try {
        const res = await fetch(`/api/image/${id}/like`, {
            method: 'POST',
            headers: { 'Content-Type': 'application/json' },
            body: JSON.stringify({ fingerprint: fp })
        });

        const data = await res.json();
        if (!res.ok) {
            showToast(dict[data.error] || dict.likeError, 'error');
            return;
        }

        const cardHeart = document.getElementById(`heart-${id}`);
        const cardCount = document.getElementById(`count-${id}`);
        if (cardHeart && cardCount) {
            if (data.liked) cardHeart.classList.add('liked');
            else cardHeart.classList.remove('liked');
            cardCount.innerText = data.count;
        }
        const img = state.images.find(i => i.id === id);
        if (img) {
            img.is_liked = data.liked;
            img.like_count = data.count;
        }
        const imgModal = document.getElementById('imageModal');
        if (imgModal.style.display === "flex" && state.images[state.currentIndex].id === id) {
            updateModalLikeState(data.liked, data.count);
        }
        showToast(data.liked ? dict.liked : dict.unliked, data.liked ? 'success' : 'info');
    } catch (e) {
        showToast(dict.likeError, 'error');
    } finally {
        state.isLikeProcessing = false;
    }
}
