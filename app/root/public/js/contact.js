export function initContact() {
    const btn = document.getElementById('contactEmailBtn');
    if (!btn) return;

    btn.addEventListener('click', (e) => {
        e.preventDefault();
        const user = '<CONTACT_USERNAME>';
        const domain = '<CONTACT_DOMAIN>';
        const address = `${user}@${domain}`;
        
        btn.innerText = address;
        window.location.href = `mailto:${address}`;
    });
}
