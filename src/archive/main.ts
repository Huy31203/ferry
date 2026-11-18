import { mount } from 'svelte';
import '../lib/theme.css';
import { applyAccent, applyOsClass } from '../lib/ipc';
import Archive from './Archive.svelte';

applyOsClass();
void applyAccent();
mount(Archive, { target: document.getElementById('app')! });
