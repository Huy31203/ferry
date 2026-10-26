import { mount } from 'svelte';
import '../lib/theme.css';
import { applyAccent, applyOsClass } from '../lib/ipc';
import Editor from './Editor.svelte';

applyOsClass();
void applyAccent();
mount(Editor, { target: document.getElementById('app')! });
