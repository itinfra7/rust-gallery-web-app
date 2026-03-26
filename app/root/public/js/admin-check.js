import { state } from './state.js';

export function handleCheck(event, index) {
    if (event.shiftKey && state.lastCheckedIndex !== null) {
        const start = Math.min(index, state.lastCheckedIndex);
        const end = Math.max(index, state.lastCheckedIndex);

        for (let i = start; i <= end; i++) {
            const checkbox = document.getElementById(`delete-check-${i}`);
            if (checkbox) {
                checkbox.checked = true;
            }
        }
    }
    state.lastCheckedIndex = index;
}

export async function deleteSelectedImages() {
    const checkboxes = document.querySelectorAll('.delete-check:checked');
    if (checkboxes.length === 0) return;
    const filenames = Array.from(checkboxes).map(cb => cb.value);

    const res = await fetch('/api/delete/bulk', {
        method: 'POST',
        headers: { 'Content-Type': 'application/json' },
        body: JSON.stringify({ filenames: filenames })
    });
    if (res.ok) {
        location.reload();
    } else {
        alert("Delete Failed");
    }
}
