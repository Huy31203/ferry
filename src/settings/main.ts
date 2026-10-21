import { mount } from 'svelte';
import '../lib/theme.css';
import { applyOsClass } from '../lib/ipc';
import Settings from './Settings.svelte';

applyOsClass();
mount(Settings, { target: document.getElementById('app')! });
