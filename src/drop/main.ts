import { mount } from 'svelte';
import '../lib/theme.css';
import { applyAccent, applyOsClass } from '../lib/ipc';
import DropZone from './DropZone.svelte';

applyOsClass();
void applyAccent();
mount(DropZone, { target: document.getElementById('app')! });
