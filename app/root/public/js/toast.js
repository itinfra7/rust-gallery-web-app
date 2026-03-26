let container = null;

function createContainer() {
    container = document.createElement('div');
    container.className = 'toast-container';
    document.body.appendChild(container);
}

export function showToast(message, type = 'info') {
    if (!container) createContainer();

    const toast = document.createElement('div');
    toast.className = `toast ${type}`;
    toast.innerText = message;

    container.appendChild(toast);

    setTimeout(() => {
        toast.classList.add('show');
    }, 0);

    setTimeout(() => {
        toast.classList.remove('show');
        setTimeout(() => {
            if (toast.parentNode) {
                toast.parentNode.removeChild(toast);
            }
        }, 300);
    }, 3000);
}
