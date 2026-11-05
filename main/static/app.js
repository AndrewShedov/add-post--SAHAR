// Disable browser's default scroll jumping so we control SPA scroll restoration precisely
if ('scrollRestoration' in history) {
    history.scrollRestoration = 'manual';
}

let savedBlogScrollY = 0;
let savedBlogFeedHtml = null;
let shouldRestoreBlogScroll = false;


function resetFormsOnLoad() {
    // 1. Reset the Create Post form on page load/refresh (prevents Firefox F5 form restoration)
    const postForm = document.getElementById('post-form');
    if (postForm) {
        postForm.reset();
        const titleField = document.getElementById('title');
        const contentField = document.getElementById('content');
        if (titleField) titleField.value = '';
        if (contentField) contentField.value = '';
    }
    updateSubmitButton();

    // 2. Reset the Edit Post form to the original database values on refresh
    resetEditFormToOriginal();
}

function updateSubmitButton() {
    const contentField = document.getElementById('content');
    const btn = document.getElementById('submit-btn');
    if (!contentField || !btn) return;

    const hasText = contentField.value.trim().length > 0;
    btn.hidden = !hasText;
    btn.disabled = !hasText;
    btn.classList.toggle('hidden', !hasText);
}

function resetEditFormToOriginal() {
    const editTitle = document.getElementById('edit-title');
    const editContent = document.getElementById('edit-content');
    if (!editTitle || !editContent) return;

    editTitle.value = editTitle.dataset.original || '';
    editContent.value = editContent.dataset.original || '';
    updateEditButtons();
}

function updateEditButtons() {
    const editTitle = document.getElementById('edit-title');
    const editContent = document.getElementById('edit-content');
    const cancelBtn = document.getElementById('cancel-edit-btn');
    const publishBtn = document.getElementById('publish-edit-btn');

    if (!editTitle || !editContent || !cancelBtn || !publishBtn) return;

    const origTitle = editTitle.dataset.original || '';
    const origContent = editContent.dataset.original || '';

    const isModified =
        editTitle.value !== origTitle || editContent.value !== origContent;
    const hasValidContent = editContent.value.trim().length > 0;

    // Show Cancel button whenever title or text differs from the original post
    cancelBtn.hidden = !isModified;
    cancelBtn.classList.toggle('hidden', !isModified);

    // Show Publish button when modified AND required * Text field is not empty
    const canPublish = isModified && hasValidContent;
    publishBtn.hidden = !canPublish;
    publishBtn.disabled = !canPublish;
    publishBtn.classList.toggle('hidden', !canPublish);
}

function updateActiveNav() {
    const currentPath = window.location.pathname;
    document.querySelectorAll('#main-nav .nav-link').forEach((link) => {
        const href = link.getAttribute('href');
        const isBlogSection =
            href === '/blog' &&
            (currentPath.startsWith('/blog') || currentPath.startsWith('/posts/'));
        link.classList.toggle('active', href === currentPath || isBlogSection);
    });
}

function autoHideToasts() {
    const toasts = document.querySelectorAll('#toast-container .toast:not([data-timer-set])');
    toasts.forEach((toast) => {
        toast.setAttribute('data-timer-set', 'true');
        setTimeout(() => {
            if (toast.parentNode) {
                toast.remove();
            }
        }, 4000);
    });
}

document.addEventListener('DOMContentLoaded', () => {
    resetFormsOnLoad();
    updateActiveNav();
});

window.addEventListener('pageshow', () => {
    resetFormsOnLoad();
    updateActiveNav();
});

document.addEventListener('input', (e) => {
    if (!e.target) return;
    if (e.target.id === 'content') {
        updateSubmitButton();
    } else if (e.target.id === 'edit-title' || e.target.id === 'edit-content') {
        updateEditButtons();
    }
});

document.addEventListener('click', (e) => {
    if (e.target.classList.contains('toast-close')) {
        const toast = e.target.closest('.toast');
        if (toast) toast.remove();
    }

    // Restore original post content when Cancel is clicked
    if (e.target.id === 'cancel-edit-btn') {
        resetEditFormToOriginal();
    }

    // Save scroll position when clicking a post or edit button in /blog.
    // Save feed HTML ONLY if Infinite Scroll has loaded more than the initial 5 posts!
    const leavingBlogToPost = e.target.closest('.post-card-link, .post-item .edit-btn');
    if (leavingBlogToPost && window.location.pathname.startsWith('/blog')) {
        savedBlogScrollY = window.scrollY;
        const feedList = document.getElementById('feed-list');
        const loadedPostsCount = feedList ? feedList.querySelectorAll('.post-item').length : 0;

        if (feedList && loadedPostsCount > 5 && feedList.innerHTML.length < 2000000) {
            savedBlogFeedHtml = feedList.innerHTML;
        } else {
            savedBlogFeedHtml = null;
        }
        shouldRestoreBlogScroll = true;

        console.log('💾 [SAVE] savedBlogScrollY:', savedBlogScrollY);
        console.log('💾 [SAVE] savedBlogFeedHtml size (chars):', savedBlogFeedHtml ? savedBlogFeedHtml.length : 0);
        console.log('💾 [SAVE] savedBlogFeedHtml content:', savedBlogFeedHtml);
    }


    // Invalidate cached feed HTML if a post is deleted, so fresh data is shown
    if (e.target.closest('.delete-btn')) {
        savedBlogFeedHtml = null;
    }

    // Clicking top navigation links (Home / Blog) resets scroll to top
    const navLink = e.target.closest('#main-nav .nav-link');
    if (navLink) {
        shouldRestoreBlogScroll = false;
        savedBlogScrollY = 0;
        savedBlogFeedHtml = null;
        document.querySelectorAll('#main-nav .nav-link').forEach((link) => {
            link.classList.toggle('active', link === navLink);
        });
    }
});

// Invalidate cached feed HTML when creating or updating a post so changes appear in the feed
document.addEventListener('submit', (e) => {
    if (e.target && (e.target.id === 'post-form' || e.target.id === 'edit-form')) {
        savedBlogFeedHtml = null;
    }
});

window.addEventListener('popstate', updateActiveNav);

// HTMX 4 lifecycle events
document.addEventListener('htmx:after:swap', (e) => {

// Remove the post card from the /blog feed immediately when deleted
    const sourceElt =
        (e.detail && e.detail.ctx && e.detail.ctx.sourceElement) || e.target;
    if (sourceElt && sourceElt.classList && sourceElt.classList.contains('delete-btn')) {
        const postItem = sourceElt.closest('.post-item');
        if (postItem) postItem.remove();
    }
// /Remove the post card from the /blog feed immediately when deleted

    const swapTarget =
        (e.detail && e.detail.ctx && e.detail.ctx.target) ||
        (e.detail && e.detail.target) ||
        e.target;

    if (swapTarget && swapTarget.id === 'main-content') {
        if (window.location.pathname.startsWith('/blog') && shouldRestoreBlogScroll) {
            const feedList = document.getElementById('feed-list');
            if (feedList && savedBlogFeedHtml) {
                feedList.innerHTML = savedBlogFeedHtml;
                if (window.htmx) {
                    window.htmx.process(feedList);
                }
            }

            savedBlogFeedHtml = null; // Immediately free RAM after restoring the feed

            const targetY = savedBlogScrollY;
            window.scrollTo(0, targetY);
            requestAnimationFrame(() => window.scrollTo(0, targetY));
            shouldRestoreBlogScroll = false;

            console.log('🎯 [RESTORE] Scrolled back to Y:', targetY);
            console.log('🧹 [CLEANUP] savedBlogFeedHtml after restore:', savedBlogFeedHtml);
        } else {
            window.scrollTo(0, 0);
        }
    }

    updateSubmitButton();
    updateEditButtons();
    updateActiveNav();
    autoHideToasts();
});

document.addEventListener('htmx:after:settle', () => {
    updateSubmitButton();
    updateEditButtons();
    updateActiveNav();
    autoHideToasts();
});

// Loader Delay
// Show initial "Loading posts... ⏳" loader when navigating to /blog

// document.addEventListener('htmx:before:request', (e) => {
//     const target =
//         (e.detail && e.detail.ctx && e.detail.ctx.target) ||
//         (e.detail && e.detail.target) ||
//         e.target;
//     const sourceElt =
//         (e.detail && e.detail.ctx && e.detail.ctx.sourceElement) || e.target;

//     if (
//         target &&
//         target.id === 'main-content' &&
//         sourceElt &&
//         sourceElt.getAttribute('href') === '/blog' &&
//         !shouldRestoreBlogScroll
//     ) {
//         target.innerHTML =
//             '<div class="blog-container"><div class="scroll-loader">Loading posts... ⏳</div></div>';
//     }
// });
 

// Show "Loading posts... ⏳" immediately when pressing F5 (refresh) on /blog

// window.addEventListener('beforeunload', () => {
//     if (window.location.pathname.startsWith('/blog')) {
//         const mainContent = document.getElementById('main-content');
//         if (mainContent) {
//             mainContent.innerHTML =
//                 '<div class="blog-container"><div class="scroll-loader">Loading posts... ⏳</div></div>';
//         }
//     }
// });
// /Loader Delay