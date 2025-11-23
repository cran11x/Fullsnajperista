// Smooth scroll for anchor links
document.querySelectorAll('a[href^="#"]').forEach(anchor => {
    anchor.addEventListener('click', function (e) {
        e.preventDefault();
        const target = document.querySelector(this.getAttribute('href'));
        if (target) {
            target.scrollIntoView({
                behavior: 'smooth',
                block: 'start'
            });
        }
    });
});

// Add scroll effect to navbar
let lastScroll = 0;
const navbar = document.querySelector('.navbar');

window.addEventListener('scroll', () => {
    const currentScroll = window.pageYOffset;
    
    if (currentScroll > 100) {
        navbar.style.background = 'rgba(15, 23, 42, 0.95)';
    } else {
        navbar.style.background = 'rgba(15, 23, 42, 0.8)';
    }
    
    lastScroll = currentScroll;
});

// Fade in animation on scroll
const observerOptions = {
    threshold: 0.1,
    rootMargin: '0px 0px -50px 0px'
};

const observer = new IntersectionObserver((entries) => {
    entries.forEach(entry => {
        if (entry.isIntersecting) {
            entry.target.style.opacity = '1';
            entry.target.style.transform = 'translateY(0)';
        }
    });
}, observerOptions);

// Observe all feature cards and download cards
document.addEventListener('DOMContentLoaded', () => {
    const cards = document.querySelectorAll('.feature-card, .download-card, .requirement-item, .step, .app-mockup');
    cards.forEach(card => {
        card.style.opacity = '0';
        card.style.transform = 'translateY(20px)';
        card.style.transition = 'opacity 0.6s ease, transform 0.6s ease';
        observer.observe(card);
    });

    // Mockup tab switching
    const mockupTabs = document.querySelectorAll('.mockup-tab');
    mockupTabs.forEach(tab => {
        tab.addEventListener('click', () => {
            mockupTabs.forEach(t => t.classList.remove('active'));
            tab.classList.add('active');
        });
    });

    // Animate feed items on scroll
    const feedItems = document.querySelectorAll('.feed-item');
    feedItems.forEach((item, index) => {
        item.style.animationDelay = `${index * 0.1}s`;
    });

    // Simulate live updates in feed (optional)
    const feedContainer = document.querySelector('.mockup-feed');
    if (feedContainer) {
        setInterval(() => {
            // Add subtle pulse animation to first item
            const firstItem = feedContainer.querySelector('.feed-item:first-child');
            if (firstItem) {
                firstItem.style.animation = 'pulse 2s ease-in-out';
                setTimeout(() => {
                    firstItem.style.animation = '';
                }, 2000);
            }
        }, 5000);
    }
});

// Add pulse animation
const style = document.createElement('style');
style.textContent = `
    @keyframes pulse {
        0%, 100% { opacity: 1; }
        50% { opacity: 0.7; }
    }
`;
document.head.appendChild(style);

