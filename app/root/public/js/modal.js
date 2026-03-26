import { state } from './state.js';
import { filterByTag } from './gallery-filters.js';
import { updateGalleryDOM } from './gallery-render.js';
import { loadComments } from './comments-view.js';
import { translations } from './lang.js';

const imgModal = document.getElementById('imageModal');
const modalImg = document.getElementById('modalImage');
const modalWrapper = document.getElementById('modalWrapper');
const modalImageArea = modalWrapper?.querySelector('.modal-image-area');
const loader = document.getElementById('imageLoader');
const modalDate = document.getElementById('modalDate');
const modalTags = document.getElementById('modalTags');
const modalHeartBtn = document.getElementById('modalHeartBtn');
const modalHeartCount = document.getElementById('modalHeartCount');
const relatedImagesGrid = document.getElementById('relatedImages');
const legalModal = document.getElementById('legalModal');

const mobileGesture = {
    tapStartX: 0,
    tapStartY: 0,
    tapCandidate: false,
    gestureMoved: false,
    touchMode: null,
    panStartX: 0,
    panStartY: 0,
    panOriginX: 0,
    panOriginY: 0,
    pinchStartDistance: 0,
    pinchStartScale: 1,
    pinchStartCenterX: 0,
    pinchStartCenterY: 0,
    pinchOriginX: 0,
    pinchOriginY: 0
};

function buildFallbackAlt(img) {
    if (img.tags && img.tags.length > 0) {
        return `Image tagged ${img.tags.slice(0, 8).join(', ')}`;
    }
    if (img.date) {
        return `Curated image on <PUBLIC_DOMAIN> uploaded on ${img.date}`;
    }
    return 'Related image';
}

function isMobileViewport() {
    return window.innerWidth <= 820;
}

function clamp(value, min, max) {
    return Math.min(max, Math.max(min, value));
}

function getTouchDistance(touches) {
    if (touches.length < 2) return 0;
    const dx = touches[0].clientX - touches[1].clientX;
    const dy = touches[0].clientY - touches[1].clientY;
    return Math.hypot(dx, dy);
}

function getTouchCenter(touches) {
    if (touches.length < 2) {
        return {
            x: touches[0]?.clientX || 0,
            y: touches[0]?.clientY || 0
        };
    }

    return {
        x: (touches[0].clientX + touches[1].clientX) / 2,
        y: (touches[0].clientY + touches[1].clientY) / 2
    };
}

function resetMobileGestureTracking() {
    mobileGesture.tapCandidate = false;
    mobileGesture.gestureMoved = false;
    mobileGesture.touchMode = null;
    mobileGesture.pinchStartDistance = 0;
}

function getFittedImageBounds(scale = state.currentScale) {
    const areaRect = modalImageArea?.getBoundingClientRect();
    if (!areaRect || !modalImg.naturalWidth || !modalImg.naturalHeight) {
        return null;
    }

    const fitScale = Math.min(
        areaRect.width / modalImg.naturalWidth,
        areaRect.height / modalImg.naturalHeight
    );

    return {
        areaWidth: areaRect.width,
        areaHeight: areaRect.height,
        renderedWidth: modalImg.naturalWidth * fitScale * scale,
        renderedHeight: modalImg.naturalHeight * fitScale * scale
    };
}

function clampMobilePan(x, y, scale = state.currentScale) {
    const bounds = getFittedImageBounds(scale);
    if (!bounds) {
        return { x, y };
    }

    const maxX = Math.max(0, (bounds.renderedWidth - bounds.areaWidth) / 2);
    const maxY = Math.max(0, (bounds.renderedHeight - bounds.areaHeight) / 2);

    return {
        x: clamp(x, -maxX, maxX),
        y: clamp(y, -maxY, maxY)
    };
}

function applyModalImageTransform() {
    if (state.isImageFocusMode && isMobileViewport()) {
        const clamped = clampMobilePan(state.currentX, state.currentY, state.currentScale);
        state.currentX = clamped.x;
        state.currentY = clamped.y;
        modalImg.style.transform = `translate(${state.currentX}px, ${state.currentY}px) scale(${state.currentScale})`;
        return;
    }

    if (state.isZoomed) {
        modalImg.style.transform = `translate(${state.currentX}px, ${state.currentY}px)`;
        return;
    }

    modalImg.style.transform = 'translate(0px, 0px) scale(1)';
}

function enterMobileImageFocus() {
    if (!isMobileViewport()) return;

    state.isZoomed = false;
    state.isImageFocusMode = true;
    state.currentScale = 1;
    state.currentX = 0;
    state.currentY = 0;
    modalImg.classList.remove('zoomed');
    modalImg.classList.add('mobile-focused');
    imgModal.classList.add('image-focus-mode');
    modalImg.style.cursor = 'grab';
    applyModalImageTransform();
}

function exitMobileImageFocus() {
    state.isImageFocusMode = false;
    state.currentScale = 1;
    state.currentX = 0;
    state.currentY = 0;
    imgModal.classList.remove('image-focus-mode');
    modalImg.classList.remove('mobile-focused');
    modalImg.style.cursor = 'zoom-in';
    resetMobileGestureTracking();
    applyModalImageTransform();
}

function handleDesktopMouseDown(e) {
    if (isMobileViewport() || !state.isZoomed) return;
    state.isDown = true;
    state.isDragging = false;
    state.startX = e.clientX;
    state.startY = e.clientY;
    const style = window.getComputedStyle(modalImg);
    const matrix = new WebKitCSSMatrix(style.transform);
    state.initialLeft = matrix.m41;
    state.initialTop = matrix.m42;
    modalImg.style.cursor = 'grabbing';
    e.preventDefault();
}

function handleDesktopMouseMove(e) {
    if (isMobileViewport() || !state.isDown || !state.isZoomed) return;
    const dx = e.clientX - state.startX;
    const dy = e.clientY - state.startY;
    if (Math.abs(dx) > 5 || Math.abs(dy) > 5) state.isDragging = true;
    state.currentX = state.initialLeft + dx;
    state.currentY = state.initialTop + dy;
    applyModalImageTransform();
}

function handleDesktopMouseUp() {
    state.isDown = false;
    if (!isMobileViewport() && state.isZoomed) {
        modalImg.style.cursor = 'grab';
    }
}

function handleMobileTouchStart(e) {
    if (!isMobileViewport()) return;

    if (e.touches.length === 2 && state.isImageFocusMode) {
        const center = getTouchCenter(e.touches);
        mobileGesture.touchMode = 'pinch';
        mobileGesture.tapCandidate = false;
        mobileGesture.gestureMoved = true;
        mobileGesture.pinchStartDistance = getTouchDistance(e.touches);
        mobileGesture.pinchStartScale = state.currentScale;
        mobileGesture.pinchStartCenterX = center.x;
        mobileGesture.pinchStartCenterY = center.y;
        mobileGesture.pinchOriginX = state.currentX;
        mobileGesture.pinchOriginY = state.currentY;
        e.preventDefault();
        return;
    }

    if (e.touches.length !== 1) {
        resetMobileGestureTracking();
        return;
    }

    const touch = e.touches[0];
    mobileGesture.tapStartX = touch.clientX;
    mobileGesture.tapStartY = touch.clientY;
    mobileGesture.tapCandidate = true;
    mobileGesture.gestureMoved = false;

    if (state.isImageFocusMode) {
        mobileGesture.touchMode = 'pan';
        mobileGesture.panStartX = touch.clientX;
        mobileGesture.panStartY = touch.clientY;
        mobileGesture.panOriginX = state.currentX;
        mobileGesture.panOriginY = state.currentY;
        e.preventDefault();
        return;
    }

    mobileGesture.touchMode = 'browse';
}

function handleMobileTouchMove(e) {
    if (!isMobileViewport()) return;

    if (state.isImageFocusMode && e.touches.length === 2 && mobileGesture.touchMode === 'pinch') {
        const distance = getTouchDistance(e.touches);
        const center = getTouchCenter(e.touches);
        if (!distance || !mobileGesture.pinchStartDistance) return;

        state.currentScale = clamp(
            mobileGesture.pinchStartScale * (distance / mobileGesture.pinchStartDistance),
            1,
            4
        );
        state.currentX = mobileGesture.pinchOriginX + (center.x - mobileGesture.pinchStartCenterX);
        state.currentY = mobileGesture.pinchOriginY + (center.y - mobileGesture.pinchStartCenterY);
        applyModalImageTransform();
        mobileGesture.tapCandidate = false;
        mobileGesture.gestureMoved = true;
        e.preventDefault();
        return;
    }

    if (e.touches.length !== 1) return;

    const touch = e.touches[0];
    const deltaFromStartX = touch.clientX - mobileGesture.tapStartX;
    const deltaFromStartY = touch.clientY - mobileGesture.tapStartY;
    if (Math.abs(deltaFromStartX) > 8 || Math.abs(deltaFromStartY) > 8) {
        mobileGesture.tapCandidate = false;
        mobileGesture.gestureMoved = true;
    }

    if (state.isImageFocusMode && mobileGesture.touchMode === 'pan') {
        state.currentX = mobileGesture.panOriginX + (touch.clientX - mobileGesture.panStartX);
        state.currentY = mobileGesture.panOriginY + (touch.clientY - mobileGesture.panStartY);
        applyModalImageTransform();
        e.preventDefault();
    }
}

function handleMobileTouchEnd(e) {
    if (!isMobileViewport()) return;

    if (state.isImageFocusMode) {
        if (e.touches.length === 1) {
            const touch = e.touches[0];
            mobileGesture.touchMode = 'pan';
            mobileGesture.panStartX = touch.clientX;
            mobileGesture.panStartY = touch.clientY;
            mobileGesture.panOriginX = state.currentX;
            mobileGesture.panOriginY = state.currentY;
            mobileGesture.tapCandidate = false;
            e.preventDefault();
            return;
        }

        if (e.touches.length === 0) {
            const touch = e.changedTouches[0];
            const deltaX = touch ? touch.clientX - mobileGesture.tapStartX : 0;
            const deltaY = touch ? touch.clientY - mobileGesture.tapStartY : 0;
            const isTap = mobileGesture.tapCandidate && Math.abs(deltaX) < 10 && Math.abs(deltaY) < 10;
            resetMobileGestureTracking();
            if (isTap) {
                exitMobileImageFocus();
                e.preventDefault();
            }
        }
        return;
    }

    if (e.touches.length > 0) return;

    const touch = e.changedTouches[0];
    const deltaX = touch ? touch.clientX - mobileGesture.tapStartX : 0;
    const deltaY = touch ? touch.clientY - mobileGesture.tapStartY : 0;
    const absX = Math.abs(deltaX);
    const absY = Math.abs(deltaY);
    const isTap = mobileGesture.tapCandidate && absX < 10 && absY < 10;
    resetMobileGestureTracking();

    if (isTap) {
        enterMobileImageFocus();
        e.preventDefault();
        return;
    }

    if (absX >= 60 && absX > absY * 1.3) {
        if (deltaX > 0) changeImage(-1);
        else changeImage(1);
        e.preventDefault();
    }
}

function handleMobileTouchCancel() {
    resetMobileGestureTracking();
}

export function initModal() {
    modalHeartBtn.addEventListener('click', (e) => {
        e.stopPropagation();
        window.toggleLike(state.images[state.currentIndex].id);
    });

    modalImg.addEventListener('click', (e) => {
        e.stopPropagation();
        if (isMobileViewport()) return;
        if (!state.isDragging) toggleZoom();
        state.isDragging = false;
    });

    modalImg.addEventListener('mousedown', handleDesktopMouseDown);
    window.addEventListener('mousemove', handleDesktopMouseMove);
    window.addEventListener('mouseup', handleDesktopMouseUp);

    modalImg.addEventListener('touchstart', handleMobileTouchStart, { passive: false });
    modalImg.addEventListener('touchmove', handleMobileTouchMove, { passive: false });
    modalImg.addEventListener('touchend', handleMobileTouchEnd, { passive: false });
    modalImg.addEventListener('touchcancel', handleMobileTouchCancel, { passive: false });

    modalWrapper.addEventListener('click', (e) => {
        if (e.target === modalWrapper || e.target.classList.contains('modal-image-area')) closeImageModal();
    });

    if (legalModal) {
        legalModal.addEventListener('click', (e) => {
            if (e.target === legalModal) closeLegalModal();
        });
        document.addEventListener('keydown', (e) => {
            if (e.key === 'Escape' && legalModal.style.display === 'flex') closeLegalModal();
        });
    }
}

export function openImageModal(index) {
    state.currentIndex = index;
    updateModalImage();
    imgModal.style.display = 'flex';
    setTimeout(() => imgModal.classList.add('show'), 10);
    document.body.style.overflow = 'hidden';
    resetZoom();
}

export function closeImageModal() {
    imgModal.classList.remove('show');
    setTimeout(() => {
        imgModal.style.display = 'none';
        modalImg.src = '';
        resetZoom();
        loader.style.display = 'none';
    }, 300);
    document.body.style.overflow = 'auto';
}

export function openLegalModal() {
    legalModal.style.display = 'flex';
    setTimeout(() => legalModal.classList.add('show'), 10);
}

export function closeLegalModal() {
    legalModal.classList.remove('show');
    setTimeout(() => {
        legalModal.style.display = 'none';
    }, 300);
}

export function changeImage(direction) {
    state.currentIndex += direction;
    if (state.currentIndex >= state.images.length) state.currentIndex = 0;
    else if (state.currentIndex < 0) state.currentIndex = state.images.length - 1;
    updateModalImage();
    resetZoom();
}

export function updateModalImage() {
    modalImg.style.display = 'none';
    loader.style.display = 'block';

    const dict = translations.en;

    const currentImg = state.images[state.currentIndex];
    modalDate.innerText = currentImg.date;
    updateModalLikeState(currentImg.is_liked, currentImg.like_count);

    modalTags.innerHTML = '';
    if (currentImg.tags && currentImg.tags.length > 0) {
        currentImg.tags.forEach(tag => {
            const span = document.createElement('span');
            span.className = 'modal-tag-item';
            span.innerText = tag;
            span.onclick = (e) => filterByTag(tag, e);
            modalTags.appendChild(span);
        });
    } else {
        modalTags.innerText = dict.noTags;
    }

    modalImg.onload = () => {
        loader.style.display = 'none';
        modalImg.style.display = 'block';
        applyModalImageTransform();
    };

    modalImg.alt = currentImg.alt_text || buildFallbackAlt(currentImg);
    modalImg.src = currentImg.media_url || ("/uploads/" + currentImg.name);
    fetchRelatedImages(currentImg.id);
    loadComments(currentImg.id);
}

async function fetchRelatedImages(id) {
    relatedImagesGrid.innerHTML = '';
    try {
        const res = await fetch(`/api/image/${id}/related`);
        if (res.ok) {
            const related = await res.json();
            if (related.length === 0) {
                relatedImagesGrid.innerHTML = '<div style="color: #666; font-size: 12px; grid-column: 1/-1; text-align: center;">No related images</div>';
                return;
            }

            related.forEach(img => {
                const link = document.createElement('a');
                link.href = img.page_url || `/image/${img.id}`;
                link.className = 'related-link';

                const thumb = document.createElement('img');
                thumb.src = img.thumb_url || `/uploads/thumbs/${img.thumb}`;
                thumb.className = 'related-img';
                thumb.alt = img.alt_text || buildFallbackAlt(img);

                link.onclick = (e) => {
                    e.stopPropagation();
                    e.preventDefault();
                    const existingIndex = state.images.findIndex(i => i.id === img.id);
                    if (existingIndex !== -1) {
                        state.currentIndex = existingIndex;
                    } else {
                        state.images = [img];
                        state.currentIndex = 0;
                        state.page = 1;
                        state.hasMore = false;
                        updateGalleryDOM([img]);
                    }
                    updateModalImage();
                    resetZoom();
                };

                link.appendChild(thumb);
                relatedImagesGrid.appendChild(link);
            });
        }
    } catch (e) {
        console.error('Failed to fetch related images');
    }
}

export function updateModalLikeState(isLiked, count) {
    if (isLiked) modalHeartBtn.classList.add('liked');
    else modalHeartBtn.classList.remove('liked');
    modalHeartCount.innerText = count;
}

function resetZoom() {
    state.isZoomed = false;
    state.isImageFocusMode = false;
    modalImg.classList.remove('zoomed');
    modalImg.classList.remove('mobile-focused');
    imgModal.classList.remove('image-focus-mode');
    state.currentScale = 1;
    state.currentX = 0;
    state.currentY = 0;
    state.isDown = false;
    state.isDragging = false;
    modalImg.style.cursor = 'zoom-in';
    resetMobileGestureTracking();
    applyModalImageTransform();
}

function toggleZoom() {
    if (isMobileViewport()) {
        if (state.isImageFocusMode) exitMobileImageFocus();
        else enterMobileImageFocus();
        return;
    }

    state.isZoomed = !state.isZoomed;
    if (state.isZoomed) {
        modalImg.classList.add('zoomed');
        modalImg.style.cursor = 'grab';
    } else {
        resetZoom();
    }
}
