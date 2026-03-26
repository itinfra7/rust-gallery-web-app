import { state } from './state.js';

const uploadModal = document.getElementById('uploadModal');
const progressContainer = document.getElementById('progressContainer');
const progressBar = document.getElementById('progressBar');
const progressText = document.getElementById('progressText');
const dropZone = document.getElementById('dropZone');
const fileInput = document.getElementById('fileInput');
const dropText = document.getElementById('dropText');

export function initUpload() {
    dropZone.addEventListener('click', () => {
        fileInput.click();
    });

    fileInput.addEventListener('change', () => {
        handleFiles(fileInput.files);
    });

    dropZone.addEventListener('dragover', (e) => {
        e.preventDefault();
        dropZone.classList.add('dragover');
    });

    dropZone.addEventListener('dragleave', () => {
        dropZone.classList.remove('dragover');
    });

    dropZone.addEventListener('drop', (e) => {
        e.preventDefault();
        dropZone.classList.remove('dragover');
        handleFiles(e.dataTransfer.files);
    });
}

export function openUploadModal() {
    uploadModal.style.display = "flex";
    setTimeout(() => uploadModal.classList.add('show'), 10);
    progressContainer.style.display = 'none';
    dropText.textContent = "Drag & Drop files here or Click";
    state.filesToUpload = [];
}

export function closeUploadModal() {
    uploadModal.classList.remove('show');
    setTimeout(() => uploadModal.style.display = "none", 300);
}

function handleFiles(files) {
    if (files.length > 0) {
        state.filesToUpload = Array.from(files);
        dropText.textContent = `${files.length} files selected`;
    }
}

export async function submitUpload() {
    if (state.filesToUpload.length === 0) return;

    progressContainer.style.display = 'block';
    
    for (let i = 0; i < state.filesToUpload.length; i++) {
        const file = state.filesToUpload[i];
        const uploadId = crypto.randomUUID();
        const formData = new FormData();
        formData.append('files', file);

        const totalPercent = Math.round(((i) / state.filesToUpload.length) * 100);
        progressBar.style.width = totalPercent + '%';
        progressText.textContent = `Uploading file ${i + 1} of ${state.filesToUpload.length}: ${file.name}`;

        const statusPoller = setInterval(async () => {
            try {
                const res = await fetch(`/api/upload/status/${uploadId}`);
                if (res.ok) {
                    const data = await res.json();
                    progressText.textContent = `[${i + 1}/${state.filesToUpload.length}] ${file.name}: ${data.status}`;
                }
            } catch (e) {
                
            }
        }, 500);

        try {
            const res = await fetch('/api/upload', {
                method: 'POST',
                headers: { 'X-Upload-ID': uploadId },
                body: formData
            });

            clearInterval(statusPoller);
            
            if (!res.ok) {
                progressText.textContent = `Error uploading ${file.name}`;
            }
        } catch (e) {
            clearInterval(statusPoller);
            progressText.textContent = `Network Error on ${file.name}`;
        }
    }

    progressBar.style.width = '100%';
    progressText.textContent = 'All transfers complete. Reloading...';
    setTimeout(() => location.reload(), 1000);
}
