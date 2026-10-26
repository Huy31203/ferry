import { mount } from 'svelte';
import '../lib/theme.css';
import { applyAccent, applyOsClass } from '../lib/ipc';
import Zones from './Zones.svelte';

applyOsClass();
void applyAccent();
mount(Zones, { target: document.getElementById('app')! });
