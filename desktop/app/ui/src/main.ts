import { mount } from 'svelte';
import './app.css';
import App from './App.svelte';
import { locale } from './lib/i18n';

document.documentElement.lang = locale;

export default mount(App, { target: document.getElementById('app')! });
