import { mount } from 'svelte';
import '../lib/theme.css';
import { applyAccent, applyOsClass } from '../lib/ipc';
import Stack from './Stack.svelte';

applyOsClass();
void applyAccent();
mount(Stack, { target: document.getElementById('app')! });
