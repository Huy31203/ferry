import { mount } from 'svelte';
import '../lib/theme.css';
import { applyAccent, applyOsClass } from '../lib/ipc';
import Compare from './Compare.svelte';

applyOsClass();
void applyAccent();
mount(Compare, { target: document.getElementById('app')! });
