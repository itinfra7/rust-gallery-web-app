import { translations } from './lang.js';
import { showToast } from './toast.js';

export function openLoginModal() {
    const loginModal = document.getElementById('loginModal');
    loginModal.style.display = "flex";
    setTimeout(() => {
        loginModal.classList.add('show');
        document.getElementById('loginId').focus();
    }, 10);
}

export function closeLoginModal() {
    const loginModal = document.getElementById('loginModal');
    loginModal.classList.remove('show');
    setTimeout(() => loginModal.style.display = "none", 300);
}

export async function submitLogin() {
    const id = document.getElementById('loginId').value;
    const pass = document.getElementById('loginPass').value;
    const key = document.getElementById('loginKey').value;

    const dict = translations.en;

    const res = await fetch('/api/login', {
        method: 'POST',
        headers: { 'Content-Type': 'application/json' },
        body: JSON.stringify({ id: id, password: pass, key: key })
    });

    const data = await res.json();
    if (data.success) {
        location.reload();
        return;
    }

    const errorCode = data.error || 'accessDenied';
    showToast(dict[errorCode] || dict.accessDenied, 'error');
}

export async function logout() {
    await fetch('/api/logout', { method: 'POST' });
    location.reload();
}
