import { state } from './state.js';
import { filterByTag } from './gallery-filters.js';
import { escapeHtml, escapeJsStr } from './utils.js';
import { setupAutocomplete } from './autocomplete-core.js';

export function editTags(id) {
    const textEl = document.getElementById(`tags-text-${id}`);
    let currentTags = [];
    textEl.querySelectorAll('.tag-item').forEach(span => currentTags.push(span.innerText));
    const currentVal = currentTags.join(', ');

    const wrapper = document.createElement('div');
    wrapper.className = 'autocomplete-wrapper';

    const input = document.createElement('input');
    input.type = 'text';
    input.className = 'edit-input';
    input.id = `edit-tags-input-${id}`;
    input.value = currentVal;

    wrapper.appendChild(input);
    input.onblur = async (e) => {
        setTimeout(() => {
            if (document.activeElement !== input) {
                saveTags(id, input.value, textEl);
            }
        }, 200);
    };

    input.onkeydown = (e) => {
        if(e.key === 'Enter') input.blur();
    };

    textEl.innerHTML = '';
    textEl.appendChild(wrapper);
    input.focus();

    setupAutocomplete(input, state.allTags, true);
}

async function saveTags(id, val, textEl) {
    try {
        const res = await fetch(`/api/image/${id}/tags`, {
            method: 'POST',
            headers: {'Content-Type': 'application/json'},
            body: JSON.stringify({ tags: val })
        });
        if (res.ok) {
            const tagsArray = val.split(',').map(s => s.trim()).filter(s=>s).sort((a, b) => a.localeCompare(b));
            const tagsHtml = tagsArray.map(tag =>
                `<span class="tag-item" onclick="filterByTag('${escapeJsStr(tag)}', event)">${escapeHtml(tag)}</span>`
            ).join('');
            textEl.innerHTML = tagsHtml || 'No Tags';

            const img = state.images.find(i => i.id === id);
            if(img) img.tags = tagsArray;

            const imgModal = document.getElementById('imageModal');
            if (imgModal.style.display === "flex" && state.images[state.currentIndex].id === id) {
                const modalTags = document.getElementById('modalTags');
                modalTags.innerHTML = '';
                if (tagsArray.length > 0) {
                    tagsArray.forEach(tag => {
                        const span = document.createElement('span');
                        span.className = 'modal-tag-item';
                        span.innerText = tag;
                        span.onclick = (e) => filterByTag(tag, e);
                        modalTags.appendChild(span);
                    });
                } else {
                    modalTags.innerText = 'No Tags';
                }
            }
        } else {
             textEl.innerText = val;
            alert('Update failed');
        }
    } catch(e) {
        alert('Network error');
    }
}
