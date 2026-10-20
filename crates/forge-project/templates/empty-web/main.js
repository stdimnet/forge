const greeting = document.querySelector("#greeting");

greeting.textContent = `Hello from {{title}}! It is ${new Date().toLocaleTimeString()}.`;
