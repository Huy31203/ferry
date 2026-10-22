import { mount } from 'svelte';
import '../lib/theme.css';
import { applyAccent, applyOsClass } from '../lib/ipc';
import Settings from './Settings.svelte';

applyOsClass();
void applyAccent();
mount(Settings, { target: document.getElementById('app')! });
